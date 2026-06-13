import React, { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { Apple, ArrowDown, BarChart2, Check, Clock, Layers, Shield, Zap } from "lucide-react";
import "./styles.css";

const SITE_URL = "https://tokenviewer.webkong.top";
const GITHUB_REPO_URL = "https://github.com/webkong/TokenViewer";
const PKG_DOWNLOAD_URL = "https://github.com/webkong/TokenViewer/releases/latest/download/TokenViewer-Installer.pkg"
const DMG_DOWNLOAD_URL = "https://github.com/webkong/TokenViewer/releases/latest/download/TokenViewer.dmg";
const LANGUAGE_STORAGE_KEY = "tokenviewer-site-language";

const copy = {
  en: {
    htmlLang: "en",
    pageTitle: "TokenViewer - AI Token Usage Tracker for macOS",
    pageDescription:
      "TokenViewer is a free, native macOS menu-bar app that tracks your AI token usage and costs across 24 providers — Claude, Codex, Kiro, Cursor, Copilot, MiMoCode and more. Local-first, no cloud required.",
    pageKeywords:
      "AI token tracker, token usage macOS, Claude token counter, Codex usage, Kiro token, AI cost tracker, menu bar app macOS",
    brandHome: "TokenViewer home",
    nav: {
      features: "Features",
      providers: "Providers",
      about: "About",
      github: "GitHub",
      download: "Download free",
      language: "中文",
      languageLabel: "Switch language",
    },
    hero: {
      title: "Track your AI token usage, all in one place",
      description:
        "TokenViewer sits in your menu bar and quietly tracks token usage and costs across 24 AI coding tools. See where your tokens go — today, this week, or over time.",
      primary: "Download for macOS",
      secondary: "See features",
      proof: ["Free", "24 providers", "Local-first"],
      previewLabel: "TokenViewer menu-bar panel preview",
    },
    features: [
      {
        icon: BarChart2,
        title: "Real-time usage dashboard",
        text: "Live token counts, cost estimates, and daily trends across all your AI tools in a single native panel.",
      },
      {
        icon: Shield,
        title: "100% local & private",
        text: "All data stays in a local SQLite file. No cloud, no account, no telemetry. Your AI usage is your business.",
      },
      {
        icon: Zap,
        title: "Native macOS performance",
        text: "Built with Rust core and SwiftUI — tiny binary, instant sync, zero browser overhead.",
      },
    ],
    story: {
      title: "One place for all your AI usage.",
      description:
        "As AI coding tools multiply, it gets hard to know which tool is costing the most or doing the most work. TokenViewer parses local logs from each tool and gives you a unified view — without touching any remote API.",
      timeline: [
        "AI tools write usage logs locally",
        "TokenViewer parses them in the background",
        "You see tokens, costs, and trends instantly",
      ],
    },
    providers: {
      title: "24 supported providers",
      description:
        "TokenViewer reads local data from the AI tools you already use — no API keys or logins required.",
      list: [
        "Claude Code", "Codex", "Kiro", "Cursor", "GitHub Copilot",
        "Gemini CLI", "Opencode", "Roocode", "Kilo Code", "Zed",
        "Goose", "Grok", "Kimi", "Craft", "OpenClaw",
        "Hermes", "Antigravity", "CodeBuddy", "WorkBuddy", "OhMyPi", "Pi",
        "KiloCLI", "EveryCode", "MiMoCode",
      ],
    },
    footer: {
      download: "Download DMG",
      copyright: "© 2026 webkong. All rights reserved.",
    },
  },
  zh: {
    htmlLang: "zh-CN",
    pageTitle: "TokenViewer - macOS AI Token 用量追踪器",
    pageDescription:
      "TokenViewer 是一款免费的原生 macOS 菜单栏应用，追踪 24 个 AI 工具的 Token 用量与费用 — 包括 Claude、Codex、Kiro、Cursor、Copilot、MiMoCode 等。本地优先，无需云端。",
    pageKeywords:
      "AI Token 追踪, Token 用量 macOS, Claude Token 统计, Codex 用量, Kiro Token, AI 费用追踪, macOS 菜单栏应用",
    brandHome: "TokenViewer 官网首页",
    nav: {
      features: "功能",
      providers: "支持工具",
      about: "关于",
      github: "GitHub",
      download: "免费下载",
      language: "EN",
      languageLabel: "切换语言",
    },
    hero: {
      title: "追踪你的 AI Token 用量，一览无余",
      description:
        "TokenViewer 静默驻守在菜单栏，实时统计 24 个 AI 编程工具的 Token 用量与费用。今天用了多少、这周花了多少，一目了然。",
      primary: "下载 macOS 版本",
      secondary: "查看功能",
      proof: ["永久免费", "支持 24 个工具", "本地优先"],
      previewLabel: "TokenViewer 菜单栏面板预览",
    },
    features: [
      {
        icon: BarChart2,
        title: "实时用量仪表盘",
        text: "在原生菜单栏面板中查看所有 AI 工具的实时 Token 数量、费用估算和每日趋势。",
      },
      {
        icon: Shield,
        title: "100% 本地存储",
        text: "所有数据保存在本地 SQLite 文件中。没有云端、没有账号、没有遥测。你的 AI 用量只属于你。",
      },
      {
        icon: Zap,
        title: "原生 macOS 极致体验",
        text: "Rust 内核 + SwiftUI 界面，极小体积，即时同步，零浏览器开销。",
      },
    ],
    story: {
      title: "所有 AI 用量，统一管理。",
      description:
        "随着 AI 编程工具越来越多，很难知道哪个工具花费最高、贡献最多。TokenViewer 解析各工具的本地日志，提供统一视图——无需调用任何远程接口。",
      timeline: [
        "AI 工具在本地写入用量日志",
        "TokenViewer 在后台自动解析",
        "你立刻看到 Token 数量、费用和趋势",
      ],
    },
    providers: {
      title: "支持 24 个工具",
      description:
        "TokenViewer 直接读取你已使用工具的本地数据——无需 API Key，无需登录。",
      list: [
        "Claude Code", "Codex", "Kiro", "Cursor", "GitHub Copilot",
        "Gemini CLI", "Opencode", "Roocode", "Kilo Code", "Zed",
        "Goose", "Grok", "Kimi", "Craft", "OpenClaw",
        "Hermes", "Antigravity", "CodeBuddy", "WorkBuddy", "OhMyPi", "Pi",
        "KiloCLI", "EveryCode", "MiMoCode",
      ],
    },
    footer: {
      download: "下载 DMG",
      copyright: "© 2026 webkong. 保留所有权利。",
    },
  },
};

function getInitialLanguage() {
  const params = new URLSearchParams(window.location.search);
  const queryLang = params.get("lang");
  if (queryLang === "zh" || queryLang === "en") return queryLang;
  const stored = window.localStorage.getItem(LANGUAGE_STORAGE_KEY);
  if (stored === "zh" || stored === "en") return stored;
  return navigator.language.toLowerCase().startsWith("zh") ? "zh" : "en";
}

function App() {
  const [language, setLanguage] = useState(getInitialLanguage);
  const t = copy[language];

  useEffect(() => {
    document.documentElement.lang = t.htmlLang;
    document.title = t.pageTitle;
    let desc = document.querySelector('meta[name="description"]');
    if (!desc) { desc = document.createElement("meta"); desc.name = "description"; document.head.appendChild(desc); }
    desc.content = t.pageDescription;
    window.localStorage.setItem(LANGUAGE_STORAGE_KEY, language);
    const url = new URL(window.location.href);
    url.searchParams.set("lang", language);
    window.history.replaceState({}, "", `${url.pathname}${url.search}${url.hash}`);
  }, [language, t]);

  return (
    <main className="site-shell">
      <div className="ambient ambient-one" />
      <div className="ambient ambient-two" />
      <Navigation t={t} onToggleLanguage={() => setLanguage(l => l === "en" ? "zh" : "en")} />
      <Hero t={t} />
      <FeatureBand t={t} />
      <ProductStory t={t} />
      <ScreenshotRow />
      <ProvidersSection t={t} />
      <Footer t={t} />
    </main>
  );
}

function Navigation({ t, onToggleLanguage }) {
  return (
    <header className="nav-wrap">
      <nav className="nav">
        <a className="brand" href="/" aria-label={t.brandHome}>
          <img src="/tokenviewer-logo.png" alt="" />
          <span>TokenViewer</span>
        </a>
        <div className="nav-links">
          <a href="/#features">{t.nav.features}</a>
          <a href="/#providers">{t.nav.providers}</a>
          <a href={GITHUB_REPO_URL} target="_blank" rel="noreferrer">{t.nav.github}</a>
        </div>
        <div className="nav-actions">
          <button className="language-toggle" type="button" onClick={onToggleLanguage} aria-label={t.nav.languageLabel}>
            {t.nav.language}
          </button>
          <a className="nav-cta" href={DMG_DOWNLOAD_URL}>{t.nav.download}</a>
        </div>
      </nav>
    </header>
  );
}

function Hero({ t }) {
  return (
    <section className="hero" id="top">
      <div className="hero-copy">
        <h1>{t.hero.title}</h1>
        <p>{t.hero.description}</p>
        <div className="hero-actions" id="download">
          <a className="primary-button" href={DMG_DOWNLOAD_URL}>
            <Apple size={19} />
            {t.hero.primary}
          </a>
          <a className="secondary-button" href="#features">
            {t.hero.secondary}
            <ArrowDown size={17} />
          </a>
        </div>
        <div className="hero-proof" aria-label="Product highlights">
          {t.hero.proof.map(item => (
            <span key={item}><Check size={16} />{item}</span>
          ))}
        </div>
      </div>
      <div className="hero-visual" aria-label={t.hero.previewLabel}>
        <DashboardPreview />
      </div>
    </section>
  );
}

function DashboardPreview() {
  return (
    <div className="panel-stage">
      <div className="panel-glow" />
      <div className="clip-panel">
        <div className="panel-header">
          <div className="panel-logo">T</div>
          <span>Token Viewer</span>
          <div className="panel-spacer" />
          <Clock size={13} style={{ opacity: 0.5 }} />
        </div>
        <div className="metric-grid">
          {[
            { label: "Today", value: "30.55M", sub: "$78.77", color: "#059669" },
            { label: "7 Days", value: "440.8M", sub: "6 active", color: "#f59e0b" },
            { label: "30 Days", value: "782.6M", sub: "~26M/day", color: "#3b82f6" },
            { label: "Total", value: "802.2M", sub: "$950.65", color: "#8b5cf6" },
          ].map(c => (
            <div className="metric-card" key={c.label} style={{ "--tint": c.color }}>
              <div className="metric-label">{c.label}</div>
              <div className="metric-value">{c.value}</div>
              <div className="metric-sub">{c.sub}</div>
            </div>
          ))}
        </div>
        <div className="chart-preview">
          <Layers size={14} style={{ opacity: 0.4 }} />
          <div className="chart-bar-group">
            {[0.3, 0.8, 0.6, 1.0, 0.4, 0.7, 0.5].map((h, i) => (
              <div key={i} className="chart-bar" style={{ "--h": h }} />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

function FeatureBand({ t }) {
  return (
    <section className="feature-band" id="features">
      {t.features.map(({ icon: Icon, title, text }) => (
        <article className="feature-card" key={title}>
          <div className="feature-icon"><Icon size={24} /></div>
          <h2>{title}</h2>
          <p>{text}</p>
        </article>
      ))}
    </section>
  );
}

function ProductStory({ t }) {
  return (
    <section className="story-section">
      <div>
        <h2>{t.story.title}</h2>
        <p>{t.story.description}</p>
      </div>
      <div className="workflow-card">
        {t.story.timeline.map((item, index) => (
          <div className="workflow-step" key={item}>
            <span>{index + 1}</span>
            <p>{item}</p>
          </div>
        ))}
      </div>
    </section>
  );
}

// Provider logo mapping
const PROVIDER_LOGOS = {
  "Claude Code": "claude-code",
  "Codex": "codex",
  "Kiro": "kiro",
  "Cursor": "cursor",
  "GitHub Copilot": "copilot",
  "Gemini CLI": "gemini",
  "Opencode": "opencode",
  "Roocode": null,
  "Kilo Code": "kilo",
  "Zed": null,
  "Goose": null,
  "Grok": "grok",
  "Kimi": "kimi",
  "Craft": null,
  "OpenClaw": "openclaw",
  "Hermes": "hermes",
  "Antigravity": "antigravity",
  "CodeBuddy": "codebuddy",
  "OhMyPi": null,
  "Pi": null,
  "KiloCLI": "kilo",
  "EveryCode": "codex",
  "MiMoCode": "mimo",
};

function ScreenshotRow() {
  return (
    <section className="screenshots-row">
      {["t1.png", "t2.png", "t3.png"].map(f => (
        <img height="100%" key={f} src={`/screenshot/${f}`} alt="TokenViewer screenshot" className="screenshot-img" />
      ))}
    </section>
  );
}

function ProvidersSection({ t }) {
  return (
    <section className="providers-section" id="providers">
      <div className="providers-copy">
        <Zap size={34} />
        <h2>{t.providers.title}</h2>
        <p>{t.providers.description}</p>
      </div>
      <div className="providers-grid">
        {t.providers.list.map(name => {
          const logoKey = PROVIDER_LOGOS[name];
          return (
            <div className="provider-chip" key={name}>
              {logoKey && (
                <img
                  src={`/logos/${logoKey}.svg`}
                  alt=""
                  width={16} height={16}
                  style={{ display: "inline-block", verticalAlign: "middle", marginRight: 5 }}
                  onError={e => { e.currentTarget.style.display = "none"; }}
                />
              )}
              {name}
            </div>
          );
        })}
      </div>
    </section>
  );
}

function Footer({ t }) {
  return (
    <footer className="footer">
      <div className="footer-main">
        <div className="brand footer-brand">
          <img src="/tokenviewer-logo.png" alt="" />
          <span>TokenViewer</span>
        </div>
        <div className="footer-links">
          <a href={GITHUB_REPO_URL} target="_blank" rel="noreferrer">GitHub</a>
          <a href={DMG_DOWNLOAD_URL}>{t.footer.download}</a>
        </div>
      </div>
      <p className="footer-copyright">{t.footer.copyright}</p>
    </footer>
  );
}

createRoot(document.getElementById("root")).render(
  <React.StrictMode><App /></React.StrictMode>
);

export default App;
