<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/images/cover-zh-TW-dark.avif">
  <img alt="Zeabur：在自己的主機上，照 Zeabur 的方式部署。從 Git、Docker Compose 或應用程式範本部署，就執行起來。" src="docs/images/cover-zh-TW-light.avif">
</picture>

<!-- Language names are written in their own script. -->
<!-- vale ZeaburZhTW.Script = NO -->
[English](README.md) · **繁體中文** · [简体中文](README.zh-CN.md)
<!-- vale ZeaburZhTW.Script = YES -->

# Zeabur 開源版

**在自己的主機上，用熟悉的 Zeabur 方式部署服務**。專案、服務、網域、環境變數和應用程式範本，都是你在 Zeabur 熟悉的用法。主機向哪家租、選哪個方案，由你自己決定。

## 用過 Zeabur？

這些和 Zeabur 一樣：

- **專案和服務**：一個專案放一組相關的服務，打開專案就看到每個服務的狀態。
- **部署方式**：從 Git、Docker Compose、Docker 映像檔或應用程式範本部署，也能直接上傳資料夾。
- **網域和環境變數**：幫服務加上網域，在服務頁修改環境變數。
- **日誌和回滾**：看服務的日誌，新版本有問題時回到上一個版本。

不一樣的地方：

- **主機是你的**：服務在你自己的主機上執行，費用依你選的雲端服務或機房計算。
- **由你維護**：升級 Zeabur 開源版、備份資料和維護主機，都由你或你的團隊負責。
- **跑在 Kubernetes 上**：Kubernetes 是在一台或多台主機上執行容器的開源系統，Zeabur 開源版就裝在它上面。全新的主機先用 [Zeatend](https://oss.zeabur.com/zh-TW/docs/zeabur/install/one-command/) 裝好 Kubernetes。

[從 Zeabur 過來](https://oss.zeabur.com/zh-TW/docs/zeabur/get-started/from-zeabur-com/)列出這些功能在文件的哪一頁，以及從哪裡開始。文件的每個步驟都附截圖。

## 能幫你做什麼

<table>
<tr>
<td width="50%"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/readme-home-dark.avif"><img src="docs/images/readme-home-light.avif" alt="首頁：整個叢集需要處理的事，依問題來源分組"></picture><br><b>需要你處理的事</b>：所有專案的問題集中在一頁，依來源分組。</td>
<td width="50%"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/readme-project-dark.avif"><img src="docs/images/readme-project-light.avif" alt="專案畫成一張圖，有服務、資料庫和它們之間的連線"></picture><br><b>每個專案一頁</b>：用圖或清單看服務，還有需要注意的事和最近的變化。</td>
</tr>
<tr>
<td width="50%"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/readme-diagnosis-dark.avif"><img src="docs/images/readme-diagnosis-light.avif" alt="一個一直當掉的服務：用白話說明原因和該怎麼做"></picture><br><b>用白話說明問題</b>：告訴你原因、該怎麼做，並附上依據。</td>
<td width="50%"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/readme-rollout-dark.avif"><img src="docs/images/readme-rollout-light.avif" alt="正在更新的服務，有用量，以及一個瀏覽器打不開的網域"></picture><br><b>更新、用量和網域</b>：更新到哪一步、CPU 和記憶體的用量與上限，以及每個網域能不能打開。</td>
</tr>
<tr>
<td width="50%"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/readme-database-dark.avif"><img src="docs/images/readme-database-light.avif" alt="PostgreSQL 資料庫：狀態、只有一個執行個體又沒有備份的警告，以及連線方式"></picture><br><b>資料庫</b>：附上連線資訊，沒有備援時會提醒你。</td>
<td width="50%"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/readme-cluster-dark.avif"><img src="docs/images/readme-cluster-light.avif" alt="叢集頁：Kubernetes 版本已停止支援，主機的記憶體和磁碟快用完"></picture><br><b>叢集</b>：Kubernetes 支援期限、主機記憶體和磁碟不足的警告、備份、通知和元件。</td>
</tr>
</table>

<sub>截圖取自內建的示範資料（`cargo run -p zeabur-api --example demo`），由 `scripts/docs-shots.sh readme` 拍攝，畫面是英文版。</sub>

## 開始使用

依手上的環境選一頁開始，每一頁都帶你走到用通行密鑰登入 Zeabur 開源版：

- 想讓 Claude Code 或 Codex 代勞：[交給 Agent 安裝或搬家](https://oss.zeabur.com/zh-TW/docs/zeabur/install/agent/)
- 有一台全新的 Linux 伺服器：[用 Zeatend 安裝](https://oss.zeabur.com/zh-TW/docs/zeabur/install/one-command/)
- 已經在跑 Kubernetes 叢集：[把 Zeabur 開源版裝進既有叢集](https://oss.zeabur.com/zh-TW/docs/zeabur/install/existing-cluster/)
- 正在用 Zeabur，想把服務搬過來：[從 Zeabur 過來](https://oss.zeabur.com/zh-TW/docs/zeabur/get-started/from-zeabur-com/)

只想在自己的電腦上看看叢集？`zeaburctl ui` 會用你的 kubeconfig（`kubectl` 連到叢集用的設定檔）在瀏覽器打開 Zeabur 開源版，見 [zeaburctl](https://oss.zeabur.com/zh-TW/docs/zeabur/reference/zeaburctl/)。

[發行檔與驗證](https://oss.zeabur.com/zh-TW/docs/zeabur/reference/releases/)說明每個版本包含哪些檔案，以及怎麼驗證。

## 從原始程式碼建置

下面的指令會下載原始程式碼，進入備好所有建置工具的 [Nix](https://nixos.org) 開發環境，再建置並啟動 Zeabur 開源版：

```sh
git clone https://github.com/zeabur-oss/zeabur && cd zeabur
nix develop
./scripts/build-ui.sh
ZEABUR_UI_DIST="$PWD/web/dist" cargo build --release --locked -p zeaburctl
./target/release/zeaburctl ui                  # 要用其他 context 時加上 --context <name>
```

`zeaburctl ui` 會監聽本機的隨機連接埠並打開瀏覽器，每個要求都用你目前的 kubeconfig context 發出。

## 參與開發

- [參與開發](CONTRIBUTING.md)：開發環境、檢查、相依套件政策和測試（英文）。
- [開發者文件](docs/internal/)：crate、測試、用語和發行（英文）。
- [`docs/`](docs/) 是文件的原始檔。這裡的頁面會發布到 [oss.zeabur.com](https://oss.zeabur.com/zh-TW/docs/zeabur/)。

## 授權

Apache-2.0。提交貢獻時須遵守 [Developer Certificate of Origin](DCO)。第三方內容的授權和聲明，見[英文版 README](README.md#license-and-credits)。
