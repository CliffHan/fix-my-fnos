use anyhow::Result;
use clap::Parser;
use tracing_subscriber::filter::EnvFilter;

mod config;
mod service;
mod utils;

#[derive(Parser, Debug)]
#[command(name = "fix-my-fnos")]
#[command(about = "监控并修复 FNOS 运行期相关问题")]
#[command(
    long_about = "监控并修复 FNOS 运行期相关问题，目前支持：\n1. 系统启动后恢复指定 docker compose 服务；\n2. 监控指定物理网卡启动消息，尝试修复 macvlan (docker & proxy) 状态；"
)]
enum Cli {
    /// 安装 systemd 服务（需要 root 权限）
    #[command(name = "install")]
    Install(Args),

    /// 卸载 systemd 服务
    #[command(name = "uninstall")]
    Uninstall,

    /// 运行监控和修复守护进程
    #[command(name = "run")]
    Run(Args),

    /// 测试 config 文件解析
    #[command(name = "test-config")]
    TestConfig(Args),

    /// 测试 docker compose 启动命令
    #[command(name = "test-compose")]
    TestCompose(Args),

    /// 运行 macvlan 网络修复命令
    #[command(name = "test-macvlan")]
    TestMacvlan(Args),
}

#[derive(Parser, Debug)]
struct Args {
    /// 配置文件
    #[arg(short, long, default_value = "/etc/fix-my-fnos/config.toml")]
    config_file: String,
}

#[allow(dead_code)]
fn init_env_tracing() {
    let env = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(env).init();
}

fn init_debug_tracing() {
    let filter = EnvFilter::new("info,fix_my_fnos=debug");
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

fn init_tracing(cli: &Cli) {
    #[cfg(debug_assertions)]
    let init_tracing_default = init_debug_tracing;
    #[cfg(not(debug_assertions))]
    let init_tracing_default = init_env_tracing;

    match cli {
        Cli::Install(_) | Cli::Uninstall | Cli::Run(_) | Cli::TestConfig(_) => init_tracing_default(),
        Cli::TestCompose(_) | Cli::TestMacvlan(_) => init_debug_tracing(),
    }
}

fn test_root(cli: &Cli) -> bool {
    match cli {
        Cli::TestConfig(_) | Cli::TestCompose(_) => true, // no need to work as root
        _ => nix::unistd::Uid::effective().is_root(),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(&cli);

    if !test_root(&cli) {
        tracing::info!("Need Root priviledge to run");
        return Ok(());
    }

    match cli {
        Cli::Install(args) => {
            let _ = load_config(&args.config_file)?;
            service::install(&args.config_file)?;
        }
        Cli::Uninstall => service::uninstall()?,
        Cli::Run(args) => service::run(load_config(&args.config_file)?).await?,
        Cli::TestConfig(args) => tracing::info!("config={:#?}", load_config(&args.config_file)?),
        Cli::TestCompose(args) => service::test_compose(load_config(&args.config_file)?).await?,
        Cli::TestMacvlan(args) => service::test_macvlan(load_config(&args.config_file)?).await?,
    }
    Ok(())
}

fn load_config(config_file: &str) -> Result<config::Config> {
    let config_str = std::fs::read_to_string(config_file)?;
    let config: config::Config = toml::from_str(&config_str)?;
    Ok(config)
}
