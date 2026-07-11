# fix-my-fnos

监控并修复 FNOS 运行期相关问题。



## 背景

使用飞牛的时候，有几个问题让我比较困扰：

1. 每次重启后，docker compose 都无法自动重启，需要手动启动。
2. 运行一段时间（通常是一天多）后，macvlan 网络就会出问题，表现为 qwrt 无法连接，以及主机上的 macvlan-proxy 消失。

稍微研究了一下。问题 1 好像是因为关机时 docker compose 为了数据安全，是正常结束的，所以不会自动重启。问题 2 似乎跟主机上的物理网卡状态重置有关系，物理网卡恢复时 macvlan 的虚拟网卡无法自动恢复。

所以我写了这个小应用，来解决这几个问题。



## 功能

- 系统启动后恢复指定 docker compose 服务
- 监控指定物理网卡状态变化，在网卡恢复 UP 时自动修复 macvlan 网络，方法为：
  - 重启指定 Docker 容器
  - 重建宿主机 proxy 网卡




## 编译

```bash
cargo build --release
```



## 配置

参见 `config.toml.template`。生成自己的 config.toml。



## 使用

可以直接从 release 下载 binary。通常用服务方式使用。

```bash
# 安装为 systemd 服务（需要 root）
sudo ./fix-my-fnos install -c /path/to/config.toml

# 启动服务
sudo systemctl start fix-my-fnos
```




## 其他子命令

| 命令 | 说明 | root |
|---|---|---|
| `install -c <config>` | 安装 systemd 服务 | Y |
| `uninstall` | 卸载 systemd 服务 | Y |
| `run -c <config>` | 运行守护进程 | Y |
| `test-config -c <config>` | 测试配置文件解析 | N |
| `test-compose -c <config>` | 测试 docker compose 启动 | N |
| `test-macvlan -c <config>` | 测试 macvlan 修复 | Y |



## 构建 Release

通过 GitHub Actions 手动触发构建和发布：

1. 进入仓库 **Actions** → **Build and Release**
2. 点击 **Run workflow**，输入 tag 名称（如 `v0.1.0`）
3. 等待构建完成后，在 Releases 页面下载二进制