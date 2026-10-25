// What each catalogue template is, in one sentence per locale. Keys are `id#hash`, the hash of the
// catalogue description the sentence summarises, as in component-summaries.ts; a changed
// description falls back to the catalogue's English until someone rewrites the sentence.
import type { Locale } from "@/i18n"
import { sourceHash } from "@/i18n/component-summaries"

type Summary = { en: string } & Partial<Record<Exclude<Locale, "en">, string>>

export const templateSummaries: Record<string, Summary> = {
  "browserless#6cec3184": {
    en: "A headless Chrome that your automation tools drive with a token.",
    "zh-TW": "讓自動化工具用權杖操作的無頭 Chrome 瀏覽器。",
    "zh-CN": "让自动化工具用令牌操作的无头 Chrome 浏览器。",
  },
  "cliproxyapi#6590260a": {
    en: "An OpenAI- and Gemini-compatible API proxy, with keys and a management page.",
    "zh-TW": "相容 OpenAI 與 Gemini 的 API 代理，附金鑰和管理頁面。",
    "zh-CN": "兼容 OpenAI 与 Gemini 的 API 代理，附密钥和管理页面。",
  },
  "dify#23cbde4e": {
    en: "A platform for building LLM apps, with PostgreSQL, a queue, a code sandbox and S3 storage.",
    "zh-TW": "打造 LLM 應用程式的平台，內含 PostgreSQL、佇列、程式碼沙箱和 S3 儲存空間。",
    "zh-CN": "构建 LLM 应用的平台，内含 PostgreSQL、队列、代码沙箱和 S3 存储。",
  },
  "error-pages#1d5a87c8": {
    en: "Shows a maintenance page and error pages for the project's domains.",
    "zh-TW": "為專案的網域顯示維護頁面和錯誤頁面。",
    "zh-CN": "为项目的域名显示维护页面和错误页面。",
  },
  "ghost#ec60c6e5": {
    en: "Ghost 6 for blogs and newsletters, with MySQL and your own SMTP server.",
    "zh-TW": "Ghost 6 部落格與電子報，搭配 MySQL 和你自己的 SMTP 伺服器。",
    "zh-CN": "Ghost 6 博客与邮件通讯，搭配 MySQL 和你自己的 SMTP 服务器。",
  },
  "gitea#c76ae580": {
    en: "Git hosting, with repositories on a volume and PostgreSQL.",
    "zh-TW": "Git 代管服務，儲存庫放在磁碟區，搭配 PostgreSQL。",
    "zh-CN": "Git 托管服务，仓库放在卷上，搭配 PostgreSQL。",
  },
  "halo#941f5a5c": {
    en: "Halo for websites and blogs, with PostgreSQL.",
    "zh-TW": "Halo 網站與部落格系統，搭配 PostgreSQL。",
    "zh-CN": "Halo 建站与博客系统，搭配 PostgreSQL。",
  },
  "hermes-agent#35c52ada": {
    en: "The Hermes AI agent with an API and a dashboard, using OpenRouter or another OpenAI-compatible provider.",
    "zh-TW": "Hermes AI 代理，附 API 和管理面板，可接 OpenRouter 或其他相容 OpenAI 的服務。",
    "zh-CN": "Hermes AI 智能体，附 API 和管理面板，可接 OpenRouter 或其他兼容 OpenAI 的服务。",
  },
  "librechat#e3d2712a": {
    en: "An AI chat app with sign-in, MongoDB, PostgreSQL for document search and Meilisearch.",
    "zh-TW": "需要登入的 AI 聊天應用程式，搭配 MongoDB、用來搜尋文件的 PostgreSQL 和 Meilisearch。",
    "zh-CN": "需要登录的 AI 聊天应用，搭配 MongoDB、用于文档检索的 PostgreSQL 和 Meilisearch。",
  },
  "litellm#0f697478": {
    en: "One proxy for many LLM providers, with keys, models and spend kept in PostgreSQL.",
    "zh-TW": "用一個代理串接多家 LLM 服務，金鑰、模型和花費存在 PostgreSQL。",
    "zh-CN": "用一个代理接入多家 LLM 服务，密钥、模型和花费存在 PostgreSQL。",
  },
  "lobechat#c3b12896": {
    en: "A chatbot interface that keeps chats in each browser, behind an access code.",
    "zh-TW": "聊天機器人介面，對話存在各自的瀏覽器裡，用存取碼保護。",
    "zh-CN": "聊天机器人界面，对话存在各自的浏览器里，用访问码保护。",
  },
  "meilisearch#756f5e16": {
    en: "A fast search engine with a master key and indexes on a volume.",
    "zh-TW": "快速的搜尋引擎，用主金鑰保護，索引存在磁碟區。",
    "zh-CN": "快速的搜索引擎，用主密钥保护，索引存在卷上。",
  },
  "memos#e5105f1c": {
    en: "A note-taking app with SQLite on a volume and an admin account ready.",
    "zh-TW": "筆記服務，資料用 SQLite 存在磁碟區，並先建好管理員帳號。",
    "zh-CN": "笔记服务，数据用 SQLite 存在卷上，并预先建好管理员账号。",
  },
  "miniflux#aea3fabb": {
    en: "A feed reader, with PostgreSQL.",
    "zh-TW": "RSS 閱讀器，搭配 PostgreSQL。",
    "zh-CN": "RSS 阅读器，搭配 PostgreSQL。",
  },
  "n8n#c5390f63": {
    en: "Workflow automation with PostgreSQL and separate code runners.",
    "zh-TW": "工作流程自動化，搭配 PostgreSQL 和獨立的程式碼執行器。",
    "zh-CN": "工作流自动化，搭配 PostgreSQL 和独立的代码执行器。",
  },
  "n8n-ai-brain#c6346add": {
    en: "Workflow automation for AI, with PostgreSQL and isolated code runners.",
    "zh-TW": "為 AI 準備的工作流程自動化，搭配 PostgreSQL 和隔離的程式碼執行器。",
    "zh-CN": "面向 AI 的工作流自动化，搭配 PostgreSQL 和隔离的代码执行器。",
  },
  "n8n-sqlite#03a02ebc": {
    en: "Workflow automation with SQLite and separate code runners.",
    "zh-TW": "工作流程自動化，資料存在 SQLite，搭配獨立的程式碼執行器。",
    "zh-CN": "工作流自动化，数据存在 SQLite，搭配独立的代码执行器。",
  },
  "new-api#cfc5c818": {
    en: "An OpenAI-compatible model gateway with PostgreSQL and an owner account ready.",
    "zh-TW": "相容 OpenAI 的模型閘道，搭配 PostgreSQL，並先建好擁有者帳號。",
    "zh-CN": "兼容 OpenAI 的模型网关，搭配 PostgreSQL，并预先建好所有者账号。",
  },
  "nginx#00d3ae44": {
    en: "An NGINX web server over HTTPS.",
    "zh-TW": "透過 HTTPS 提供服務的 NGINX 網頁伺服器。",
    "zh-CN": "通过 HTTPS 提供服务的 NGINX 网页服务器。",
  },
  "ntfy#4fac2453": {
    en: "A push notification server where topics need an account and messages stay on a volume.",
    "zh-TW": "推播通知伺服器，主題需要帳號，訊息存在磁碟區。",
    "zh-CN": "推送通知服务器，主题需要账号，消息存在卷上。",
  },
  "ollama#8cfe24c9": {
    en: "Runs local models on the CPU; its API is reachable only inside the cluster.",
    "zh-TW": "在 CPU 上執行本機模型，API 只能從叢集內連線。",
    "zh-CN": "在 CPU 上运行本地模型，API 只能从集群内访问。",
  },
  "open-webui#e4b0c18b": {
    en: "A chat interface for LLMs with sign-in; its licence asks you to keep its branding.",
    "zh-TW": "需要登入的 LLM 聊天介面，授權要求保留它的品牌標示。",
    "zh-CN": "需要登录的 LLM 聊天界面，许可要求保留其品牌标识。",
  },
  "openclaw#5a9f1bd0": {
    en: "A personal AI assistant that connects to WhatsApp, Telegram, Slack, Discord and more.",
    "zh-TW": "個人 AI 助理，可以串接 WhatsApp、Telegram、Slack、Discord 等通訊軟體。",
    "zh-CN": "个人 AI 助手，可以连接 WhatsApp、Telegram、Slack、Discord 等通讯软件。",
  },
  "openclaw-browser-sandbox#db663c08": {
    en: "A headless Chrome that OpenClaw or other services drive through the DevTools protocol.",
    "zh-TW": "無頭 Chrome 沙箱，讓 OpenClaw 或其他服務透過 DevTools 協定操作。",
    "zh-CN": "无头 Chrome 沙箱，供 OpenClaw 或其他服务通过 DevTools 协议操作。",
  },
  "openlist#9f5df137": {
    en: "File listing and WebDAV, with FFmpeg and Aria2 downloads.",
    "zh-TW": "檔案列表與 WebDAV，內含 FFmpeg 和 Aria2 下載。",
    "zh-CN": "文件列表与 WebDAV，内含 FFmpeg 和 Aria2 下载。",
  },
  "postgresql-pgvector#57e52404": {
    en: "A PostgreSQL 18 database with pgvector.",
    "zh-TW": "已啟用 pgvector 的 PostgreSQL 18 資料庫。",
    "zh-CN": "已启用 pgvector 的 PostgreSQL 18 数据库。",
  },
  "qdrant#a1a5dfde": {
    en: "A vector database with an API key and data on a volume.",
    "zh-TW": "向量資料庫，用 API 金鑰保護，資料存在磁碟區。",
    "zh-CN": "向量数据库，用 API 密钥保护，数据存在卷上。",
  },
  "searxng#6fd85fb4": {
    en: "A private metasearch engine.",
    "zh-TW": "注重隱私的整合搜尋引擎。",
    "zh-CN": "注重隐私的元搜索引擎。",
  },
  "sillytavern#333fcb8c": {
    en: "An LLM chat frontend behind a password, with conversations kept on a volume.",
    "zh-TW": "用密碼保護的 LLM 聊天前端，對話存在磁碟區。",
    "zh-CN": "用密码保护的 LLM 聊天前端，对话存在卷上。",
  },
  "umami#100f405f": {
    en: "Website analytics, with PostgreSQL.",
    "zh-TW": "網站流量分析，搭配 PostgreSQL。",
    "zh-CN": "网站流量分析，搭配 PostgreSQL。",
  },
  "uptime-kuma#3294a748": {
    en: "Uptime monitoring, with its data on a volume.",
    "zh-TW": "監控服務是否正常上線，資料存在磁碟區。",
    "zh-CN": "监控服务是否正常在线，数据存在卷上。",
  },
  "vaultwarden#fec71e80": {
    en: "A Bitwarden-compatible password manager; sign-up is closed and the admin invites people.",
    "zh-TW": "相容 Bitwarden 的密碼管理器，不開放註冊，由管理員邀請使用者。",
    "zh-CN": "兼容 Bitwarden 的密码管理器，不开放注册，由管理员邀请用户。",
  },
  "wordpress#278ee3b0": {
    en: "WordPress, with MariaDB and an owner account ready.",
    "zh-TW": "WordPress 網站，搭配 MariaDB，並先建好擁有者帳號。",
    "zh-CN": "WordPress 网站，搭配 MariaDB，并预先建好所有者账号。",
  },
}

export const templateSummaryKey = (id: string, description: string) =>
  `${id}#${sourceHash(description)}`

/** The template's sentence in `locale`, else in English; the catalogue's description when that
 * changed. */
export function templateSummary(id: string, description: string, locale: Locale): string {
  const summary = templateSummaries[templateSummaryKey(id, description)]
  return summary ? (summary[locale] ?? summary.en) : description
}
