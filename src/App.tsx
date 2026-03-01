import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  Globe,
  Server,
  Wifi,

  XCircle,
  Loader2,
  ChevronDown,
  ChevronUp,
  Terminal,
  Rocket,
  Key,
  Save,
  Sparkles,
  RefreshCw,
} from "lucide-react";
import { EmbeddedTerminal } from "./EmbeddedTerminal";

// ============================================================
// 类型定义
// ============================================================

/** 安装流程的状态机 */
type InstallPhase =
  | "idle"          // 空闲，等待用户点击
  | "checking"      // 正在检测网络和依赖
  | "installing"    // 正在安装（依赖或 Claude Code）
  | "configuring"   // 安装完成，等待配置 API Key
  | "done"          // 全部完成
  | "error";        // 出错

/** 网络模式 */
type NetworkMode = "unknown" | "global" | "mirror";



// ============================================================
// 主应用组件
// ============================================================
function App() {
  // ---- 状态 ----
  const [phase, setPhase] = useState<InstallPhase>("idle");
  const [networkMode, setNetworkMode] = useState<NetworkMode>("unknown");
  const [platform, setPlatform] = useState<string>("windows");
  const [logs, setLogs] = useState<string[]>([]);
  const [logsOpen, setLogsOpen] = useState(false);
  const [provider, setProvider] = useState("claude");
  const [apiKey, setApiKey] = useState("");
  const [modelName, setModelName] = useState("");
  const [saveKeyMsg, setSaveKeyMsg] = useState("");
  const [errorMsg, setErrorMsg] = useState("");
  const [isTerminalOpen, setIsTerminalOpen] = useState(false);

  const logEndRef = useRef<HTMLDivElement>(null);

  // 日志滚动到底部
  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [logs]);

  // ---- 工具函数 ----
  /** 追加日志 */
  const appendLog = useCallback((line: string) => {
    setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] ${line}`]);
  }, []);

  // ============================================================
  // 初始化及环境检测
  // ============================================================
  
  // ---- 监听后端日志事件 ----
  useEffect(() => {
    const unlisten = listen<string>("log-event", (event) => {
      appendLog(event.payload);
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  // ---- 初始化：获取平台、检测环境、加载配置 ----
  useEffect(() => {
    // 获取平台信息
    invoke<string>("get_platform").then(setPlatform);

    // 加载已保存的 API Key
    invoke<{ provider: string | null; api_key: string | null; model_name: string | null }>("load_api_key")
      .then((cfg) => {
        if (cfg.provider) setProvider(cfg.provider);
        if (cfg.api_key) setApiKey(cfg.api_key);
        if (cfg.model_name) setModelName(cfg.model_name);
      })
      .catch((e) => console.error("加载 API Key 失败:", e));

    // 检测是否已安装 Claude
    invoke<boolean>("check_claude_installed")
      .then((installed) => {
        if (installed) {
          // 如果已安装，可直接进入 configuring 阶段（跳过安装）
          setPhase("configuring");
          appendLog("ℹ️ 检测到系统已安装 Claude Code，跳过自动配置安装阶段。");
        }
      })
      .catch((e) => console.error("检测 Claude 安装状态失败:", e));
    // 监听后端安装日志流
    const unlistenLogs = listen<string>("install-log", (event) => {
      appendLog(event.payload);
    });

    return () => {
      unlistenLogs.then((f) => f());
    };
  }, []);

  // ============================================================
  // 核心安装流程
  // ============================================================
  const startInstall = async () => {
    // 手动点击重新检测时，重置状态
    setPhase("checking");
    setErrorMsg("");
    appendLog("--- 开始新的安装流程 ---");
    
    // 如果想要全新安装，需要手动先校验一下
    try {
      const isInstalled = await invoke<boolean>("check_claude_installed");
      if (isInstalled) {
         appendLog("⚠️ 检测到您已经安装了 Claude Code。即将跳过安装依赖...");
         setPhase("configuring");
         return;
      }
    } catch {}

    appendLog("🔍 正在检查系统环境与网络连通性...");

    try {
      // ===== Step 1: 网络探测 =====
      appendLog("🌐 正在探测网络环境...");
      const connectivity = await invoke<{ is_global: boolean }>("check_global_connectivity");
      const mode: NetworkMode = connectivity.is_global ? "global" : "mirror";
      setNetworkMode(mode);
      appendLog(
        mode === "global"
          ? "✅ 网络畅通 → 使用 Global 模式（官方源）"
          : "⚠️ 国际网络受限 → 使用 Mirror 模式（国内镜像）"
      );
      // ===== 绿色便携打包安装流程 =====
      
      // 第 1 步：检测内置 Portable Node.js 环境是否就绪
      appendLog("🔍 正在检查内置 Portable 运行时...");
      const hasNode = await invoke<boolean>("check_portable_node");
      
      if (!hasNode) {
        throw new Error(
          "当前安装包体积异常，未检测到内置封包的 'Portable/Node' 免安装环境，请开发者确保编译前置入了核心资源！"
        );
      }
      
      appendLog("✅ 已检测到内置便携式 Node.js 环境。");

      setPhase("installing");

      // 获取当前 Portable 目录以便执行内部 npm
      const portableDir = await invoke<string>("get_portable_dir");
      const claudeBinDir = `${portableDir}\\ClaudeBin`;
      const nodeDir = `${portableDir}\\Node`;
      const npmCmd = platform === "windows" ? `${nodeDir}\\npm.cmd` : `${nodeDir}/npm`;
      
      appendLog(`🛠️ 即将在隔离环境中部署 Claude Code...`);
      appendLog(`   [DEBUG] portable目录: ${portableDir}`);
      appendLog(`   [DEBUG] node目录: ${nodeDir}`);
      appendLog(`   [DEBUG] npmCmd路径: ${npmCmd}`);
      appendLog(`   [DEBUG] claudeBin目录: ${claudeBinDir}`);

      // 使用后端原生的 install_claude_portable 命令进行安装
      // 此命令会自动处理 PATH 环境变量和编码容错 (UFT-8 Lossy)
      await invoke("install_claude_portable", { isMirror: mode === "mirror" });

      appendLog("🎉 便携版 Claude Code 部署完成！");
      setPhase("configuring");
    } catch (err: any) {
      setErrorMsg(String(err));
      appendLog(`❌ 安装失败: ${err}`);
      setPhase("error");
    }
  };



  // ---- 保存 API Key ----
  const handleSaveApiKey = async () => {
    try {
      const msg = await invoke<string>("save_api_key", {
        config: {
          provider: provider,
          api_key: apiKey || null,
          model_name: modelName || null,
        },
      });
      setSaveKeyMsg(msg);
      appendLog(`🔑 ${msg}`);
      setPhase("done");
    } catch (err: any) {
      setSaveKeyMsg(`保存失败: ${err}`);
    }
  };
  return (
    <div className="h-full flex flex-col bg-bg-primary overflow-hidden relative">
      {/* ===== 顶部标题栏 ===== */}
      <header className="flex items-center justify-between px-6 py-4 border-b border-border">
        <div className="flex items-center gap-3">
          <Sparkles className="w-6 h-6 text-accent" />
          <h1 className="text-xl font-bold bg-gradient-to-r from-accent to-purple-400 bg-clip-text text-transparent">
            Claude Code 安装器
          </h1>
        </div>

        {/* 网络状态指示灯 */}
        <NetworkIndicator mode={networkMode} />
      </header>

      {/* ===== 主内容区 ===== */}
      <main className="flex-1 overflow-y-auto p-6 space-y-6">

        {/* 安装主面板 */}
        <div className="animate-fade-in-up">
          {phase === "idle" && (
            <div className="flex flex-col items-center gap-6 py-8">
              <p className="text-text-secondary text-center max-w-md">
                一键检测系统环境并自动安装 Claude Code CLI 工具，开启 AI 编程之旅。
              </p>
              <button
                onClick={startInstall}
                className="group relative px-10 py-4 bg-accent hover:bg-accent-hover text-white font-bold text-lg rounded-xl
                           transition-all duration-300 cursor-pointer
                           shadow-[0_0_20px_var(--color-accent-glow)]
                           hover:shadow-[0_0_30px_var(--color-accent-glow),0_0_60px_rgba(99,102,241,0.15)]
                           hover:scale-105 active:scale-95"
                style={{ animation: "pulse-glow 3s ease-in-out infinite" }}
              >
                <span className="flex items-center gap-3">
                  <Rocket className="w-6 h-6 group-hover:rotate-12 transition-transform" />
                  一键开启 AI 编程
                </span>
              </button>
            </div>
          )}

          {(phase === "checking" || phase === "installing") && (
            <div className="flex flex-col items-center gap-4 py-6">
              <Loader2 className="w-10 h-10 text-accent animate-spin-slow" />
              <p className="text-text-secondary">
                {phase === "checking" ? "正在检测系统环境..." : "正在安装，请耐心等待..."}
              </p>
            </div>
          )}

          {phase === "error" && (
            <div className="bg-error/10 border border-error/30 rounded-xl p-6 text-center">
              <XCircle className="w-10 h-10 text-error mx-auto mb-3" />
              <p className="text-error font-medium mb-2">安装过程中出现错误</p>
              <p className="text-text-muted text-sm mb-4">{errorMsg}</p>
              <button
                onClick={() => { setPhase("idle"); setErrorMsg(""); }}
                className="px-6 py-2 bg-bg-card border border-border rounded-lg
                           hover:border-accent transition-colors cursor-pointer text-text-primary"
              >
                重试
              </button>
            </div>
          )}

          {/* API Key 配置面板 */}
          {(phase === "configuring" || phase === "done") && (
            <ApiKeyPanel
              provider={provider}
              apiKey={apiKey}
              modelName={modelName}
              onProviderChange={(p) => {
                setProvider(p);
                // 切换提供商时自动填充默认模型
                const found = PROVIDERS.find((x) => x.id === p);
                if (found?.defaultModel) setModelName(found.defaultModel);
              }}
              onApiKeyChange={setApiKey}
              onModelNameChange={setModelName}
              onSave={handleSaveApiKey}
              saveMsg={saveKeyMsg}
              isDone={phase === "done"}
              onSkip={() => setPhase("done")}
              onLaunchTerminal={() => setIsTerminalOpen(true)}
              onReinstall={() => {
                setPhase("idle");
                startInstall();
              }}
            />
          )}
        </div>
      </main>

      {/* ===== 底部日志面板 ===== */}
      <LogTerminal
        logs={logs}
        isOpen={logsOpen}
        onToggle={() => setLogsOpen(!logsOpen)}
        logEndRef={logEndRef}
      />

      {/* PTY 内置交互终端 */}
      {isTerminalOpen && (
        <EmbeddedTerminal onClose={() => setIsTerminalOpen(false)} />
      )}
    </div>
  );
}

// ============================================================
// 子组件：网络状态指示灯
// ============================================================
function NetworkIndicator({ mode }: { mode: NetworkMode }) {
  const configs = {
    unknown: { color: "bg-text-muted", glow: "", label: "未检测", Icon: Wifi },
    global: { color: "bg-success", glow: "shadow-[0_0_8px_var(--color-success-glow)]", label: "Global 直连", Icon: Globe },
    mirror: { color: "bg-warning", glow: "shadow-[0_0_8px_var(--color-warning-glow)]", label: "Mirror 镜像", Icon: Server },
  };
  const cfg = configs[mode];

  return (
    <div className="flex items-center gap-2 px-3 py-1.5 bg-bg-card rounded-full border border-border">
      <span
        className={`w-2.5 h-2.5 rounded-full ${cfg.color} ${cfg.glow}`}
        style={mode !== "unknown" ? { animation: "blink 2s ease-in-out infinite" } : {}}
      />
      <cfg.Icon className="w-4 h-4 text-text-secondary" />
      <span className="text-xs text-text-secondary font-medium">{cfg.label}</span>
    </div>
  );
}



// ============================================================
// 子组件：API Key 配置面板
// ============================================================
// 提供商定义
const PROVIDERS = [
  { id: "claude",   label: "Claude 官方",      desc: "Anthropic 官方 API",        placeholder: "sk-ant-...",           defaultModel: "" },
  { id: "zhipu",    label: "智谱 AI",          desc: "智谱开放平台",              placeholder: "输入智谱 API Key...",     defaultModel: "glm-4-plus" },
  { id: "volcano",  label: "火山引擎 (豆包)", desc: "字节跳动 火山方舟",          placeholder: "输入火山 API Key...",     defaultModel: "doubao-1.5-pro-256k" },
  { id: "bailian",  label: "阿里百炼",          desc: "阿里云 DashScope",         placeholder: "输入百炼 API Key...",     defaultModel: "qwen-plus" },
  { id: "openai",   label: "OpenAI",           desc: "OpenAI 官方 API",           placeholder: "sk-...",               defaultModel: "gpt-4o" },
  { id: "gemini",   label: "Google Gemini",    desc: "Google AI Studio",          placeholder: "AIza...",              defaultModel: "gemini-2.0-flash" },
];

function ApiKeyPanel({
  provider,
  apiKey,
  modelName,
  onProviderChange,
  onApiKeyChange,
  onModelNameChange,
  onSave,
  saveMsg,
  isDone,
  onSkip,
  onLaunchTerminal,
  onReinstall,
}: {
  provider: string;
  apiKey: string;
  modelName: string;
  onProviderChange: (v: string) => void;
  onApiKeyChange: (v: string) => void;
  onModelNameChange: (v: string) => void;
  onSave: () => void;
  saveMsg: string;
  isDone: boolean;
  onSkip: () => void;
  onLaunchTerminal: () => void;
  onReinstall: () => void;
}) {
  const currentProvider = PROVIDERS.find((p) => p.id === provider) || PROVIDERS[0];
  return (
    <div className="bg-bg-card border border-border rounded-xl p-6 space-y-5 animate-fade-in-up">
      {isDone ? (
        <div className="flex flex-col items-center gap-3 py-4">
          <Rocket className="w-14 h-14 text-accent" />
          <h3 className="text-xl font-bold text-text-primary">Claude Code 就绪</h3>
          <p className="text-text-secondary text-sm text-center">
            可以直接在下方启动内置交互终端，或在系统终端中运行 <code className="text-accent bg-bg-terminal px-2 py-0.5 rounded">claude</code>。
          </p>
          <p className="text-text-muted text-xs">
            当前提供商：<span className="text-accent font-medium">{currentProvider.label}</span>
          </p>
          <button
              onClick={onLaunchTerminal}
              className="mt-6 flex items-center gap-2 px-6 py-3 bg-accent hover:bg-accent-hover text-white font-medium rounded-xl transition-all shadow-[0_0_15px_rgba(99,102,241,0.2)] hover:shadow-[0_0_25px_rgba(99,102,241,0.4)] cursor-pointer"
          >
              <Terminal className="w-5 h-5" />
              启动交互终端 (运行 Claude Code)
          </button>
          <button
              onClick={onReinstall}
              className="mt-2 text-sm text-text-muted hover:text-text-primary underline cursor-pointer"
          >
              需要重新检测和安装？点击这里重新运行环境诊断。
          </button>
        </div>
      ) : (
        <>
          <div className="flex items-center gap-2">
            <Key className="w-5 h-5 text-accent" />
            <h3 className="text-lg font-semibold text-text-primary">配置 API 提供商（可选）</h3>
          </div>
          <p className="text-text-muted text-sm">
            选择 AI 提供商并填写对应的 API Key，也可以稍后在 Claude Code 中设置。
          </p>

          <div className="space-y-4">
            {/* 提供商选择器 */}
            <div>
              <label className="block text-sm text-text-secondary mb-2">选择提供商</label>
              <div className="grid grid-cols-3 gap-2">
                {PROVIDERS.map((p) => (
                  <button
                    key={p.id}
                    onClick={() => onProviderChange(p.id)}
                    className={`px-3 py-2.5 rounded-lg border text-sm font-medium transition-all cursor-pointer
                      ${
                        provider === p.id
                          ? "border-accent bg-accent/10 text-accent shadow-[0_0_10px_rgba(99,102,241,0.15)]"
                          : "border-border bg-bg-input text-text-secondary hover:border-border-focus hover:text-text-primary"
                      }`}
                  >
                    <div className="font-semibold">{p.label}</div>
                    <div className="text-xs opacity-70 mt-0.5">{p.desc}</div>
                  </button>
                ))}
              </div>
            </div>

            {/* API Key 输入框 */}
            <div>
              <label className="block text-sm text-text-secondary mb-1">
                {currentProvider.label} API Key
              </label>
              <input
                type="password"
                value={apiKey}
                onChange={(e) => onApiKeyChange(e.target.value)}
                placeholder={currentProvider.placeholder}
                className="w-full px-4 py-2.5 bg-bg-input border border-border rounded-lg
                           text-text-primary placeholder-text-muted text-sm
                           focus:outline-none focus:border-border-focus focus:ring-1 focus:ring-border-focus
                           transition-colors"
              />
            </div>

            {/* 模型选择器（非 Claude 官方时显示） */}
            {provider !== "claude" && (
              <ModelSelector
                provider={provider}
                apiKey={apiKey}
                modelName={modelName}
                onModelNameChange={onModelNameChange}
                defaultModel={currentProvider.defaultModel}
              />
            )}
          </div>

          {saveMsg && (
            <p className={`text-sm ${saveMsg.includes("失败") ? "text-error" : "text-success"}`}>
              {saveMsg}
            </p>
          )}

          <div className="flex gap-3">
            <button
              onClick={onSave}
              className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5
                         bg-accent hover:bg-accent-hover text-white font-medium rounded-lg
                         transition-colors cursor-pointer"
            >
              <Save className="w-4 h-4" />
              保存配置
            </button>
            <button
              onClick={onSkip}
              className="px-6 py-2.5 bg-bg-input border border-border rounded-lg
                         text-text-secondary hover:text-text-primary transition-colors cursor-pointer"
            >
              跳过
            </button>
          </div>
        </>
      )}
    </div>
  );
}

// ============================================================
// 子组件：模型选择器（带 API 自动获取）
// ============================================================
function ModelSelector({
  provider,
  apiKey,
  modelName,
  onModelNameChange,
  defaultModel,
}: {
  provider: string;
  apiKey: string;
  modelName: string;
  onModelNameChange: (v: string) => void;
  defaultModel: string;
}) {
  const [models, setModels] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const handleFetch = async () => {
    if (!apiKey) {
      setError("请先填写 API Key");
      return;
    }
    setLoading(true);
    setError("");
    try {
      const list = await invoke<string[]>("fetch_models", {
        provider,
        apiKey,
      });
      setModels(list);
      if (list.length === 0) {
        setError("未获取到模型，请检查 API Key");
      }
    } catch (err: any) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div>
      <div className="flex items-center justify-between mb-1">
        <label className="text-sm text-text-secondary">模型名称</label>
        <button
          onClick={handleFetch}
          disabled={loading}
          className="flex items-center gap-1 text-xs text-accent hover:text-accent-hover transition-colors cursor-pointer
                     disabled:opacity-50 disabled:cursor-not-allowed"
        >
          <RefreshCw className={`w-3.5 h-3.5 ${loading ? "animate-spin" : ""}`} />
          {loading ? "加载中..." : "获取模型列表"}
        </button>
      </div>

      {models.length > 0 ? (
        <select
          value={modelName}
          onChange={(e) => onModelNameChange(e.target.value)}
          className="w-full px-4 py-2.5 bg-bg-input border border-border rounded-lg
                     text-text-primary text-sm appearance-none cursor-pointer
                     focus:outline-none focus:border-border-focus focus:ring-1 focus:ring-border-focus
                     transition-colors"
        >
          <option value="">请选择模型...</option>
          {models.map((m) => (
            <option key={m} value={m}>
              {m}
            </option>
          ))}
        </select>
      ) : (
        <input
          type="text"
          value={modelName}
          onChange={(e) => onModelNameChange(e.target.value)}
          placeholder={defaultModel || "输入模型名称或点击“获取模型列表”"}
          className="w-full px-4 py-2.5 bg-bg-input border border-border rounded-lg
                     text-text-primary placeholder-text-muted text-sm
                     focus:outline-none focus:border-border-focus focus:ring-1 focus:ring-border-focus
                     transition-colors"
        />
      )}

      {error && <p className="text-xs text-error mt-1">{error}</p>}
      {!error && models.length === 0 && (
        <p className="text-xs text-text-muted mt-1">默认: {defaultModel || "无"}</p>
      )}
    </div>
  );
}

// ============================================================
// 子组件：折叠式终端日志窗口
// ============================================================
function LogTerminal({
  logs,
  isOpen,
  onToggle,
  logEndRef,
}: {
  logs: string[];
  isOpen: boolean;
  onToggle: () => void;
  logEndRef: React.RefObject<HTMLDivElement | null>;
}) {
  return (
    <div className="border-t border-border bg-bg-secondary">
      {/* 折叠按钮 */}
      <button
        onClick={onToggle}
        className="w-full flex items-center justify-between px-4 py-2
                   hover:bg-bg-card/50 transition-colors cursor-pointer"
      >
        <span className="flex items-center gap-2 text-sm text-text-secondary">
          <Terminal className="w-4 h-4" />
          详细日志
          {logs.length > 0 && (
            <span className="text-xs bg-bg-card px-2 py-0.5 rounded-full text-text-muted">
              {logs.length}
            </span>
          )}
        </span>
        {isOpen ? (
          <ChevronDown className="w-4 h-4 text-text-muted" />
        ) : (
          <ChevronUp className="w-4 h-4 text-text-muted" />
        )}
      </button>

      {/* 日志内容 - 模拟终端 */}
      {isOpen && (
        <div className="h-52 overflow-y-auto bg-bg-terminal px-4 py-3 font-mono text-xs leading-relaxed">
          {logs.length === 0 ? (
            <p className="text-terminal-dim italic">等待操作...</p>
          ) : (
            logs.map((line, i) => (
              <div
                key={i}
                className={`${
                  line.includes("✅") || line.includes("🎉")
                    ? "text-terminal-green"
                    : line.includes("❌")
                    ? "text-error"
                    : line.includes("⚠️")
                    ? "text-warning"
                    : "text-text-secondary"
                }`}
              >
                {line}
              </div>
            ))
          )}
          {/* 闪烁的终端光标 */}
          <span
            className="inline-block w-2 h-3.5 bg-terminal-green ml-1"
            style={{ animation: "cursor-blink 1s step-end infinite" }}
          />
          <div ref={logEndRef} />
        </div>
      )}
    </div>
  );
}

export default App;
