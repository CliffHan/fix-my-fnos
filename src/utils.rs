use anyhow::{Result, anyhow};
use bollard::Docker;
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::query_parameters::RestartContainerOptions;
use futures_util::{StreamExt, TryStreamExt};
use rtnetlink::packet_route::link::MacVlanMode;
use rtnetlink::{Handle, LinkMacVlan, LinkUnspec, RouteMessageBuilder};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub async fn is_docker_running() -> bool {
    let docker = match Docker::connect_with_defaults() {
        Ok(d) => d,
        Err(_) => return false,
    };
    docker.ping().await.is_ok()
}

pub async fn is_container_running(docker: &Docker, container: &str) -> Result<bool> {
    let container_info = docker.inspect_container(container, None).await?;
    Ok(container_info.state.and_then(|s| s.running).unwrap_or(false))
}

pub async fn ping_gateway(docker: &Docker, container: &str, gateway: &str) -> Result<bool> {
    let exec = docker
        .create_exec(
            container,
            CreateExecOptions {
                cmd: Some(vec!["ping", "-c", "1", gateway]),
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                ..Default::default()
            },
        )
        .await?;

    match docker.start_exec(&exec.id, None).await? {
        StartExecResults::Attached { mut output, .. } => {
            while output.next().await.is_some() {}
            let inspect = docker.inspect_exec(&exec.id).await?;
            Ok(inspect.exit_code == Some(0))
        }
        _ => Ok(false),
    }
}

pub async fn restart_container(docker: &Docker, container: &str) -> Result<()> {
    docker.restart_container(container, Some(RestartContainerOptions { signal: None, t: Some(10) })).await?;
    Ok(())
}

pub async fn get_link_index(handle: &Handle, name: &str) -> Result<u32> {
    let mut links = handle.link().get().match_name(name.to_string()).execute();
    match links.try_next().await? {
        Some(link) => Ok(link.header.index),
        None => Err(anyhow!("interface {} not found", name)),
    }
}

// pub async fn delete_link(handle: &Handle, name: &str) -> Result<()> {
//     let mut links = handle.link().get().match_name(name.to_string()).execute();
//     if let Some(link) = links.try_next().await? {
//         handle.link().del(link.header.index).execute().await?;
//     }
//     Ok(())
// }

pub async fn create_macvlan_proxy(handle: &Handle, parent_index: u32, name: &str) -> Result<u32> {
    handle.link().add(LinkMacVlan::new(name, parent_index, MacVlanMode::Bridge).build()).execute().await?;
    let index = get_link_index(handle, name).await?;
    Ok(index)
}

pub async fn bring_up_proxy(handle: &Handle, index: u32, ip: IpAddr, prefix: u8) -> Result<()> {
    handle.address().add(index, ip, prefix).execute().await?;
    handle.link().change(LinkUnspec::new_with_index(index).up().build()).execute().await?;
    Ok(())
}

pub async fn reset_container_route(handle: &Handle, proxy_index: u32, container_ip: IpAddr) -> Result<()> {
    let (del_message, add_message) = match container_ip {
        IpAddr::V4(ip) => (
            RouteMessageBuilder::<Ipv4Addr>::new().destination_prefix(ip, 32).build(),
            RouteMessageBuilder::<Ipv4Addr>::new().destination_prefix(ip, 32).output_interface(proxy_index).build(),
        ),

        IpAddr::V6(ip) => (
            RouteMessageBuilder::<Ipv6Addr>::new().destination_prefix(ip, 128).build(),
            RouteMessageBuilder::<Ipv6Addr>::new().destination_prefix(ip, 128).output_interface(proxy_index).build(),
        ),
    };
    let _ = handle.route().del(del_message).execute().await;
    handle.route().add(add_message).execute().await?;
    Ok(())
}
