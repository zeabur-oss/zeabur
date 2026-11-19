<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/images/cover-zh-CN-dark.avif">
  <img alt="Zeabur：在自己的服务器上，照 Zeabur 的方式部署。从 Git、Docker Compose 或应用模板部署，就运行起来。" src="docs/images/cover-zh-CN-light.avif">
</picture>

<!-- Language names are written in their own script. -->
<!-- vale ZeaburZhCN.Script = NO -->
[English](README.md) · [繁體中文](README.zh-TW.md) · **简体中文**
<!-- vale ZeaburZhCN.Script = YES -->

# Zeabur

**在自己的服务器上，用熟悉的 Zeabur 方式部署服务。**Zeabur 是开源软件。项目、服务、域名、环境变量和应用模板，都是你在 zeabur.com 上熟悉的用法。服务器向哪家租、选哪个套餐，由你自己决定。

## 用过 zeabur.com？

这些和 zeabur.com 一样：

- **项目和服务**：一个项目放一组相关的服务，打开项目就能看到每个服务的状态。
- **部署方式**：从 Git、Docker Compose、Docker 镜像或应用模板部署，也可以直接上传文件夹。
- **域名和环境变量**：给服务加上域名，在服务页修改环境变量。
- **日志和回滚**：查看服务的日志，新版本有问题时回到上一个版本。

不一样的地方：

- **服务器是你的**：服务在你自己的服务器上运行，费用按你选的云厂商或机房计算。
- **由你维护**：升级 Zeabur、备份数据和维护服务器，都由你或你的团队负责。
- **目前需要先有 Kubernetes 集群**：Kubernetes 是在一台或多台服务器上运行容器的开源系统，Zeabur 就装在它上面。在全新服务器上用一条命令完成安装的功能还在开发中，进展见 [#430](https://github.com/zeabur-oss/zeabur/issues/430)。

[文档](https://oss.zeabur.com/zh-CN/docs/zeabur/)一步步讲解安装、部署和配置，每一步都附有截图。

## 能帮你做什么

<table>
<tr>
<td width="50%"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/readme-home-dark.avif"><img src="docs/images/readme-home-light.avif" alt="首页：整个集群需要处理的事，按问题来源分组"></picture><br><b>需要你处理的事</b>：所有项目的问题集中在一页，按来源分组。</td>
<td width="50%"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/readme-project-dark.avif"><img src="docs/images/readme-project-light.avif" alt="项目画成一张图，有服务、数据库和它们之间的连接"></picture><br><b>每个项目一页</b>：用图或列表看服务，还有需要注意的事和最近的变化。</td>
</tr>
<tr>
<td width="50%"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/readme-diagnosis-dark.avif"><img src="docs/images/readme-diagnosis-light.avif" alt="一个反复崩溃的服务：说明原因和处理方法"></picture><br><b>说清楚问题</b>：告诉你原因、该怎么做，并附上依据。</td>
<td width="50%"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/readme-rollout-dark.avif"><img src="docs/images/readme-rollout-light.avif" alt="正在更新的服务，有用量，以及一个浏览器打不开的域名"></picture><br><b>更新、用量和域名</b>：更新到哪一步、CPU 和内存的用量与上限，以及每个域名能不能打开。</td>
</tr>
<tr>
<td width="50%"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/readme-database-dark.avif"><img src="docs/images/readme-database-light.avif" alt="PostgreSQL 数据库：状态、只有一个实例又没有备份的警告，以及连接方式"></picture><br><b>数据库</b>：附上连接信息，没有备用实例时会提醒你。</td>
<td width="50%"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/readme-cluster-dark.avif"><img src="docs/images/readme-cluster-light.avif" alt="集群页：Kubernetes 版本已停止支持，服务器的内存和磁盘快用完"></picture><br><b>集群</b>：Kubernetes 支持期限、服务器内存和磁盘不足的警告、备份、通知和组件。</td>
</tr>
</table>

<sub>截图取自内置的演示数据（`cargo run -p zeabur-api --example demo`），由 `scripts/docs-shots.sh readme` 拍摄，界面是英文版。</sub>

## 开始使用

目前的安装方式，是用 Helm 把 Zeabur 装进已有的 Kubernetes 集群。Helm 是 Kubernetes 的安装工具。登录由 Zeabur Identity 负责，所以要先装好它。

[将 Zeabur 安装到已有集群](https://oss.zeabur.com/zh-CN/docs/zeabur/install/existing-cluster/)列出了要准备的东西和每个步骤。

只想在自己的电脑上看看集群？`zeaburctl ui` 会用你的 kubeconfig（`kubectl` 连接集群用的配置文件）在浏览器里打开 Zeabur，见 [zeaburctl](https://oss.zeabur.com/zh-CN/docs/zeabur/reference/zeaburctl/)。

[发行文件与验证](https://oss.zeabur.com/zh-CN/docs/zeabur/reference/releases/)说明每个版本包含哪些文件，以及如何验证。

## 从源码构建

下面的命令会下载源码，进入备好所有构建工具的 [Nix](https://nixos.org) 开发环境，然后构建并启动 Zeabur：

```sh
git clone https://github.com/zeabur-oss/zeabur && cd zeabur
nix develop
./scripts/build-ui.sh
ZEABUR_UI_DIST="$PWD/web/dist" cargo build --release --locked -p zeaburctl
./target/release/zeaburctl ui                  # 要用其他 context 时加上 --context <name>
```

`zeaburctl ui` 会监听本机的随机端口并打开浏览器，每个请求都用你当前的 kubeconfig context 发出。

## 参与开发

- [参与开发](CONTRIBUTING.md)：开发环境、检查、依赖策略和测试（英文）。
- [开发者文档](docs/internal/)：crate、测试、用语和发布（英文）。
- [`docs/`](docs/) 是文档的源文件。这里的页面会发布到 [oss.zeabur.com](https://oss.zeabur.com/zh-CN/docs/zeabur/)。

## 许可证

Apache-2.0。提交贡献时须遵守 [Developer Certificate of Origin](DCO)。第三方内容的许可证和声明，见[英文版 README](README.md#license-and-credits)。
