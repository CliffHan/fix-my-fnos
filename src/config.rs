use std::net::IpAddr;

use ipnet::IpNet;
use serde::Deserialize;
use serde_inline_default::serde_inline_default;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub startup: StartupConfig,
    pub compose: DockerComposeConfig,
    pub macvlan: MacvlanConfig,
}

#[serde_inline_default]
#[derive(Debug, Deserialize)]
pub struct StartupConfig {
    /// 应用启动时等待 Docker 服务的超时时间（秒）
    #[serde_inline_default(300)]
    pub startup_timeout: u64,

    /// 应用启动时检测 Docker 服务的间隔时间（秒）
    #[serde_inline_default(30)]
    pub startup_interval: u64,
}

#[derive(Debug, Deserialize)]
pub struct DockerComposeConfig {
    #[serde(default)]
    pub directories: Vec<String>,
}

#[serde_inline_default]
#[derive(Clone, Debug, Deserialize)]
pub struct MacvlanConfig {
    /// 要监听的主机网卡，如 bond1、eth0
    pub interface: String,

    /// macvlan 代理网卡名
    #[serde_inline_default(String::from("macvlan-proxy"))]
    pub proxy_interface: String,

    /// macvlan 代理网卡 IP 及网段，如 192.168.1.3/24
    pub proxy_cidr: IpNet,

    /// 使用 macvlan 的 Docker 容器名
    #[serde_inline_default(String::from("qwrt"))]
    pub container_name: String,

    /// 使用 macvlan 的 Docker 容器固定 IP
    pub container_ip: IpAddr,

    /// 局域网网关 IP，用于测试网络联通状态
    pub gateway_ip: IpAddr,
}
