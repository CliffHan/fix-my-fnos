use crate::config::*;
use crate::utils::*;
use anyhow::{Result, anyhow};
use bollard::Docker;
use futures_util::StreamExt;
use rtnetlink::packet_core::NetlinkMessage;
use rtnetlink::packet_core::NetlinkPayload;
use rtnetlink::packet_route::RouteNetlinkMessage;
use rtnetlink::packet_route::link::{LinkAttribute, LinkFlags};
use rtnetlink::sys::SocketAddr;
use rtnetlink::{Handle, MulticastGroup, new_connection, new_multicast_connection};
use std::net::IpAddr;
use tokio::sync::mpsc::{Sender, channel};

type MonitorMessage = ();
type NetlinkMessages = futures_channel::mpsc::UnboundedReceiver<(NetlinkMessage<RouteNetlinkMessage>, SocketAddr)>;

pub async fn run(config: Config) -> Result<()> {
    tracing::debug!("service::run(), config={:#?}", config);
    wait_for_docker_service(config.startup.startup_timeout, config.startup.startup_interval).await?;
    run_docker_compose(&config.compose).await;
    let (repair_tx, mut repair_rx) = channel::<MonitorMessage>(1);
    let interface = config.macvlan.interface.clone();
    let macvlan_config = config.macvlan;
    let docker = Docker::connect_with_defaults()?;
    let (connection, handle, messages) = new_multicast_connection(&[MulticastGroup::Link])?;
    tokio::spawn(connection);
    tokio::spawn(async move {
        monitor(interface, repair_tx, messages).await;
    });
    // TODO: 1st try_send after boot?

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("received ctrl-c, exiting");
                break;
            }
            v = repair_rx.recv() => {
                if v.is_none() {
                    return Err(anyhow!("monitor task ended unexpectedly"));
                }
                tracing::debug!("service::run(), received {} up event", &macvlan_config.interface);
                match repair_macvlan(&macvlan_config, &docker, &handle).await {
                    Ok(_) => tracing::info!("Successfully repaired macvlan"),
                    Err(e) => tracing::error!("Failed to repair macvlan: {:#}", e),
                }
            }

        }
    }
    Ok(())
}

pub fn install(config_file: &str) -> Result<()> {
    tracing::debug!("service::install(), config_file={}", config_file);
    todo!();
}

pub fn uninstall() -> Result<()> {
    tracing::debug!("service::uninstall()");
    todo!();
}

pub async fn test_compose(config: Config) -> Result<()> {
    tracing::debug!("service::test_compose(), config={:#?}", config);
    wait_for_docker_service(config.startup.startup_timeout, config.startup.startup_interval).await?;
    run_docker_compose(&config.compose).await;
    Ok(())
}

pub async fn test_macvlan(config: Config) -> Result<()> {
    tracing::debug!("service::test_macvlan(), config={:#?}", config);
    wait_for_docker_service(config.startup.startup_timeout, config.startup.startup_interval).await?;
    let docker = Docker::connect_with_defaults()?;
    let (connection, handle, _) = new_connection()?;
    tokio::spawn(connection);
    repair_macvlan(&config.macvlan, &docker, &handle).await?;
    Ok(())
}

async fn wait_for_docker_service(startup_timeout: u64, startup_interval: u64) -> Result<()> {
    use std::time::Duration;
    use tokio::time::sleep;

    let mut elapsed = 0;
    while elapsed < startup_timeout {
        if is_docker_running().await {
            tracing::info!("Docker service is running");
            return Ok(());
        }
        tracing::info!("Docker service is not running, waiting for {} seconds...", startup_interval);
        sleep(Duration::from_secs(startup_interval)).await;
        elapsed += startup_interval;
    }
    Err(anyhow!("Docker service did not start within {} seconds", startup_timeout))
}

async fn run_docker_compose(compose_config: &DockerComposeConfig) {
    tracing::debug!("service::run_docker_compose(), compose_config={:#?}", compose_config);
    for dir in compose_config.directories.iter() {
        tracing::info!("Running docker-compose in directory: {}", dir);
        let output_result = tokio::process::Command::new("docker")
            .args(["compose", "--project-directory", dir, "up", "-d"])
            .output()
            .await;
        match output_result {
            Ok(output) => {
                if output.status.success() {
                    tracing::info!("Successfully ran docker-compose in directory: {}", dir);
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    tracing::error!("docker-compose failed in directory: {}. Error: {}", dir, stderr);
                }
            }
            Err(_) => tracing::info!("Failed to run docker-compose in directory: {}", dir),
        }
    }
}

async fn monitor(interface: String, repair_tx: Sender<MonitorMessage>, mut messages: NetlinkMessages) {
    tracing::debug!("service::monitor(), interface={}", interface);
    while let Some((message, _)) = messages.next().await {
        // tracing::debug!("service::monitor(), received message: {:?}", message);
        let payload = match message.payload {
            NetlinkPayload::InnerMessage(inner) => inner,
            _ => continue,
        };
        if let RouteNetlinkMessage::NewLink(link) = payload {
            let mut ifname = None;
            for attr in link.attributes {
                if let LinkAttribute::IfName(name) = attr {
                    ifname = Some(name);
                }
            }
            if ifname.as_deref() == Some(interface.as_str()) && link.header.flags.contains(LinkFlags::Up) {
                let _ = repair_tx.try_send(());
            }
        }
    }
    tracing::debug!("service::monitor(), finished");
}

async fn repair_macvlan(macvlan_config: &MacvlanConfig, docker: &Docker, handle: &Handle) -> Result<()> {
    tracing::debug!("service::repair_macvlan(), macvlan_config={:#?}", macvlan_config);
    let container = macvlan_config.container_name.as_str();
    let gateway = macvlan_config.gateway_ip.to_string();
    if let Err(e) = restart_macvlan_container(docker, container, &gateway).await {
        tracing::error!("Failed to restart container {}: {}", container, e);
    }

    let parent = macvlan_config.interface.as_str();
    let proxy = macvlan_config.proxy_interface.as_str();
    let proxy_ip = macvlan_config.proxy_cidr.addr();
    let prefix = macvlan_config.proxy_cidr.prefix_len();
    let container_ip = macvlan_config.container_ip;
    if let Err(e) = reset_macvlan_proxy(handle, parent, proxy, proxy_ip, prefix, container_ip).await {
        tracing::error!("Failed to reset macvlan proxy {}: {:#}", proxy, e);
    }
    Ok(())
}

async fn restart_macvlan_container(docker: &Docker, container: &str, gateway: &str) -> Result<()> {
    tracing::debug!("service::restart_macvlan_container(), container={}, gateway={}", container, gateway);
    let container_is_running = is_container_running(docker, container).await?;
    let container_macvlan_is_working = match container_is_running {
        true => ping_gateway(docker, container, gateway).await?,
        false => false,
    };
    if !container_macvlan_is_working {
        restart_container(docker, container).await?;
    } else {
        tracing::debug!("service::restart_macvlan_container(), container is working, skipping restart");
    }
    Ok(())
}

async fn reset_macvlan_proxy(
    handle: &Handle,
    parent: &str,
    proxy: &str,
    proxy_ip: IpAddr,
    prefix: u8,
    container_ip: IpAddr,
) -> Result<()> {
    tracing::debug!("service::reset_macvlan_proxy(), parent={}, proxy={}, proxy_ip={}", parent, proxy, proxy_ip);
    tracing::debug!("service::reset_macvlan_proxy(), prefix={}, container_ip={}", prefix, container_ip);
    if get_link_index(handle, proxy).await.is_ok() {
        tracing::debug!("service::reset_macvlan_proxy(), {} already exists, skipping reset", proxy);
        return Ok(());
    }
    // let _ = delete_link(handle, proxy).await;
    let parent_index = get_link_index(handle, parent).await?;
    let proxy_index = create_macvlan_proxy(handle, parent_index, proxy).await?;
    bring_up_proxy(handle, proxy_index, proxy_ip, prefix).await?;
    reset_container_route(handle, proxy_index, container_ip).await?;
    Ok(())
}
