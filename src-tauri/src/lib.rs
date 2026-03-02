use serde::{Deserialize, Serialize};
use std::fs;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::timeout;

// ============================================================
// 数据结构定义
// ============================================================

/// 网络连通性检测结果
#[derive(Serialize, Deserialize)]
pub struct ConnectivityResult {
    /// 是否能直连国际网络（true = Global 模式，false = Mirror 镜像模式）
    pub is_global: bool,
}

/// 依赖检测结果
#[derive(Serialize, Deserialize)]
pub struct DependencyResult {
    /// 依赖名称
    pub name: String,
    /// 是否已安装
    pub installed: bool,
    /// 版本号（若已安装）
    pub version: Option<String>,
}

/// API Key 配置（通用多提供商模式）
#[derive(Serialize, Deserialize)]
pub struct ApiKeyConfig {
    /// 提供商标识: claude | zhipu | volcano | bailian | openai | gemini
    pub provider: Option<String>,
    /// API Key
    pub api_key: Option<String>,
    /// 模型名称
    pub model_name: Option<String>,
}

/// 根据提供商获取对应的 Base URL（None 表示使用默认/官方地址）
fn get_base_url_for_provider(provider: &str) -> Option<&'static str> {
    match provider {
        "zhipu"    => Some("https://open.bigmodel.cn/api/anthropic"),
        "volcano"  => Some("https://ark.cn-beijing.volces.com/api/v3/"),
        "bailian"  => Some("https://dashscope.aliyuncs.com/compatible-mode/v1/"),
        "openai"   => Some("https://api.openai.com/v1/"),
        "gemini"   => Some("https://generativelanguage.googleapis.com/v1beta/"),
        "claude" | _ => None, // Claude 官方不需要设置 Base URL
    }
}

/// 尝试在指定超时内连接目标地址
async fn try_connect(addr: &str, timeout_duration: Duration) -> bool {
    timeout(timeout_duration, TcpStream::connect(addr))
        .await
        .map_or(false, |r| r.is_ok())
}

// ============================================================
// Tauri 命令：智能网络探测
// ============================================================
/// 并行尝试连接 Google DNS (8.8.8.8:53) 和 google.com:443
/// 若 3 秒内任一连接成功则判定为 Global 模式
/// 超时则判定为 Mirror 模式（需使用国内镜像）
#[tauri::command]
async fn check_global_connectivity() -> ConnectivityResult {
    let dur = Duration::from_secs(3);

    // 并行发起两个 TCP 连接探测，任一成功即判定为 Global
    let (dns_ok, web_ok) = tokio::join!(
        try_connect("8.8.8.8:53", dur),
        try_connect("google.com:443", dur)
    );

    ConnectivityResult { is_global: dns_ok || web_ok }
}

// ============================================================
// Tauri 命令：依赖检测
// ============================================================
/// 检测指定命令行工具是否已安装
/// 通过执行 `<tool> --version` 并捕获输出来判断
/// 支持检测：node, git, npm
#[tauri::command]
async fn check_dependency(name: String) -> DependencyResult {
    let cmd = match name.as_str() {
        "node" => "node",
        "git" => "git",
        "npm" => "npm",
        _ => {
            return DependencyResult {
                name,
                installed: false,
                version: None,
            }
        }
    };

    // 执行 `<tool> --version` 获取版本信息
    // 注意：Windows 上某些工具（如 npm）是 .cmd 批处理文件，
    // 需要通过 cmd /c 来执行，否则 Command::new 找不到
    let output = if cfg!(target_os = "windows") {
        std::process::Command::new("cmd")
            .args(["/c", cmd, "--version"])
            .output()
    } else {
        std::process::Command::new(cmd)
            .arg("--version")
            .output()
    };

    match output {
        Ok(out) if out.status.success() => {
            let version_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
            // 提取版本号（通常格式为 "v18.17.0" 或 "git version 2.42.0"）
            // 优先匹配纯数字开头的 token（如 "2.42.0"），再匹配 "v" 开头后跟数字的 token（如 "v18.17.0"）
            let version = version_str
                .split_whitespace()
                .find(|s| s.chars().next().map_or(false, |c| c.is_ascii_digit()))
                .or_else(|| version_str.split_whitespace().find(|s| {
                    s.starts_with('v') && s.len() > 1 && s.chars().nth(1).map_or(false, |c| c.is_ascii_digit())
                }))
                .unwrap_or(&version_str)
                .trim_start_matches('v')
                .to_string();

            DependencyResult {
                name,
                installed: true,
                version: Some(version),
            }
        }
        _ => DependencyResult {
            name,
            installed: false,
            version: None,
        },
    }
}

use std::path::PathBuf;

// ============================================================
// 绿色便携打包配置 (离线内置 Bundled Resources)
// ============================================================
/// 获取绿色便携版根目录的字符串路径。该目录通过 tauri.conf.json 的 resources 配置被打包进来。
#[tauri::command]
fn get_portable_dir(app: AppHandle) -> Result<String, String> {
    use tauri::Manager;
    
    // Tauri v2 中的 Path API 变更：先获取 resource_dir，然后再跟进我们要寻找的 `Portable` 文件夹
    let resource_dir = app.path()
        .resource_dir()
        .map_err(|e| format!("核心组件缺失，无法定位内置便携包资源: {}", e))?;

    let resource_path = resource_dir.join("Portable");

    if !resource_path.exists() {
        return Err("内部错误：已加载资源路径但该环境实体不存在。请确保您下载的是完整打包含有 Node 的免安装整合包！".to_string());
    }

    Ok(resource_path.to_string_lossy().to_string()
        .trim_start_matches(r"\\?\")
        .to_string())
}

/// 检查是否已包含预装的 Node.js
#[tauri::command]
fn check_portable_node(app: AppHandle) -> Result<bool, String> {
    // 假如 `get_portable_dir` 返回错误，说明整合包全毁了，直接 false
    let root_str = match get_portable_dir(app) {
        Ok(dir) => dir,
        Err(_) => return Ok(false)
    };
    let root = PathBuf::from(root_str);
    
    let node_dir = root.join("Node");
    let node_exe = if cfg!(target_os = "windows") {
        node_dir.join("node.exe")
    } else {
        node_dir.join("bin").join("node")
    };
    Ok(node_exe.exists())
}

/// 核心安装命令：使用内置 Node 在沙盒环境中安装 Claude Code
#[tauri::command]
async fn install_claude_portable(app: AppHandle, is_mirror: bool) -> Result<(), String> {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};

    let root_str = get_portable_dir(app.clone())?;
    let root = PathBuf::from(&root_str);
    let node_dir = root.join("Node");
    let claude_bin_dir = root.join("ClaudeBin");
    
    // 确保路径存在
    if !claude_bin_dir.exists() {
        fs::create_dir_all(&claude_bin_dir).map_err(|e| format!("创建沙盒目录失败: {}", e))?;
    }

    let mut npm_cmd = if cfg!(target_os = "windows") {
        node_dir.join("npm.cmd")
    } else {
        node_dir.join("bin").join("npm")
    };

    // Windows 兼容性增强：如果根目录没找到 npm.cmd，尝试在 node_modules 深度路径中找
    if cfg!(target_os = "windows") && !npm_cmd.exists() {
        let alt_npm = node_dir.join("node_modules").join("npm").join("bin").join("npm.cmd");
        if alt_npm.exists() {
            npm_cmd = alt_npm;
        }
    }

    // 配置环境变量，确保 npm 能找到配套的 node
    let mut path_env = node_dir.to_string_lossy().to_string();
    if !cfg!(target_os = "windows") {
        path_env = node_dir.join("bin").to_string_lossy().to_string();
    }
    
    if let Ok(current_path) = std::env::var("PATH") {
        let sep = if cfg!(target_os = "windows") { ";" } else { ":" };
        path_env = format!("{}{}{}", path_env, sep, current_path);
    }

    // 1. 若为镜像模式，先设置 registry
    if is_mirror {
        let mut reg_cmd = Command::new(&npm_cmd);
        reg_cmd.args(["config", "set", "registry", "https://registry.npmmirror.com"]);
        reg_cmd.env("PATH", &path_env);
        let _ = reg_cmd.status();
    }

    // 2. 执行安装命令
    let mut child = Command::new(&npm_cmd)
        .args([
            "install",
            "@anthropic-ai/claude-code",
            "--prefix",
            &claude_bin_dir.to_string_lossy(),
        ])
        .env("PATH", &path_env)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动安装进程失败: {}", e))?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let app_clone = app.clone();

    // 辅助函数：安全地向前端发送日志（处理非 UTF-8 编码）
    fn emit_log(app: &AppHandle, prefix: &str, data: &[u8]) {
        let line = String::from_utf8_lossy(data);
        if !line.trim().is_empty() {
            let _ = app.emit("install-log", format!("{} {}", prefix, line.trim()));
        }
    }

    // 在独立线程中读取日志，避免阻塞
    let app_out = app_clone.clone();
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.split(b'\n') {
            if let Ok(l) = line {
                emit_log(&app_out, " │", &l);
            }
        }
    });

    let app_err = app_clone.clone();
    std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.split(b'\n') {
            if let Ok(l) = line {
                emit_log(&app_err, " │ [stderr]", &l);
            }
        }
    });

    let status = child.wait().map_err(|e| format!("等待安装进程结束失败: {}", e))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("安装进程退出，退出码: {:?}", status.code()))
    }
}

// ============================================================
// Tauri 命令：保存 API Key 配置
// ============================================================

/// 将用户选择的提供商和 API Key 写入 ~/.claude/config.json
#[tauri::command]
async fn save_api_key(config: ApiKeyConfig) -> Result<String, String> {
    let home_dir = dirs::home_dir().ok_or("无法获取用户主目录".to_string())?;
    let config_dir = home_dir.join(".claude");
    if !config_dir.exists() {
        fs::create_dir_all(&config_dir).map_err(|e| format!("创建配置目录失败: {}", e))?;
    }

    let config_path = config_dir.join("config.json");

    let mut existing: serde_json::Value = if config_path.exists() {
        let content = fs::read_to_string(&config_path).map_err(|e| format!("读取配置文件失败: {}", e))?;
        serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    // 保存提供商标识
    if let Some(provider) = &config.provider {
        existing["provider"] = serde_json::json!(provider);
    }

    // 保存 API Key
    if let Some(key) = &config.api_key {
        if !key.is_empty() {
            existing["apiKey"] = serde_json::json!(key);
        }
    }

    // 保存模型名称
    if let Some(model) = &config.model_name {
        if !model.is_empty() {
            existing["modelName"] = serde_json::json!(model);
        }
    }

    let json_str = serde_json::to_string_pretty(&existing)
        .map_err(|e| format!("序列化配置失败: {}", e))?;
    fs::write(&config_path, json_str).map_err(|e| format!("写入配置文件失败: {}", e))?;

    Ok(format!("配置已保存至 {}", config_path.display()))
}

#[tauri::command]
async fn load_api_key() -> Result<ApiKeyConfig, String> {
    let home_dir = dirs::home_dir().ok_or("无法获取用户主目录".to_string())?;
    let config_path = home_dir.join(".claude").join("config.json");

    let mut config = ApiKeyConfig {
        provider: None,
        api_key: None,
        model_name: None,
    };

    if config_path.exists() {
        if let Ok(content) = fs::read_to_string(&config_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(p) = json.get("provider").and_then(|v| v.as_str()) {
                    config.provider = Some(p.to_string());
                }
                if let Some(key) = json.get("apiKey").and_then(|v| v.as_str()) {
                    config.api_key = Some(key.to_string());
                }
                if let Some(model) = json.get("modelName").and_then(|v| v.as_str()) {
                    config.model_name = Some(model.to_string());
                }
            }
        }
    }

    Ok(config)
}

/// 获取提供商的模型列表端点
fn get_models_url_for_provider(provider: &str) -> Option<String> {
    match provider {
        "zhipu"    => Some("https://open.bigmodel.cn/api/paas/v4/models".to_string()),
        "volcano"  => Some("https://ark.cn-beijing.volces.com/api/v3/models".to_string()),
        "bailian"  => Some("https://dashscope.aliyuncs.com/compatible-mode/v1/models".to_string()),
        "openai"   => Some("https://api.openai.com/v1/models".to_string()),
        "gemini"   => Some("https://generativelanguage.googleapis.com/v1beta/models".to_string()),
        _ => None,
    }
}

/// 从提供商 API 获取可用模型列表
#[tauri::command]
async fn fetch_models(provider: String, api_key: String) -> Result<Vec<String>, String> {
    let url = get_models_url_for_provider(&provider)
        .ok_or("该提供商不支持获取模型列表".to_string())?;

    let client = reqwest::Client::new();

    // Gemini 使用 query param 传 key，其他用 Bearer token
    let response = if provider == "gemini" {
        client.get(format!("{}?key={}", url, api_key))
            .send().await
            .map_err(|e| format!("请求失败: {}", e))?
    } else {
        client.get(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .send().await
            .map_err(|e| format!("请求失败: {}", e))?
    };

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("API 返回错误 {}: {}", status, body));
    }

    let body: serde_json::Value = response.json().await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    let mut models = Vec::new();

    // Gemini 格式: { "models": [{"name": "models/gemini-pro", ...}] }
    if provider == "gemini" {
        if let Some(arr) = body.get("models").and_then(|v| v.as_array()) {
            for item in arr {
                if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                    // 去掉 "models/" 前缀
                    let clean = name.strip_prefix("models/").unwrap_or(name);
                    models.push(clean.to_string());
                }
            }
        }
    } else {
        // OpenAI 兼容格式: { "data": [{"id": "model-name", ...}] }
        if let Some(arr) = body.get("data").and_then(|v| v.as_array()) {
            for item in arr {
                if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                    models.push(id.to_string());
                }
            }
        }
    }

    models.sort();
    Ok(models)
}

// ============================================================
// Tauri 命令：检测 Claude Code 是否已安装
// ============================================================
/// 检测系统是否已经安装了 Claude Code
#[tauri::command]
async fn check_claude_installed(app: AppHandle) -> Result<bool, String> {
    let root_str = get_portable_dir(app)?;
    let root = PathBuf::from(root_str);
    // 便携版全局路径我们定义为 Portable/ClaudeBin
    let claude_bin = root.join("ClaudeBin");
    if cfg!(target_os = "windows") {
        let claude_exe = claude_bin.join("claude.exe");
        let claude_cmd = claude_bin.join("claude.cmd");
        if claude_exe.exists() || claude_cmd.exists() {
            return Ok(true);
        }
    } else {
        let claude_bin_sh = claude_bin.join("bin").join("claude");
        if claude_bin_sh.exists() {
            return Ok(true);
        }
    }

    Ok(false)
}

// ============================================================
// Tauri 命令：获取系统平台信息
// ============================================================
/// 返回当前操作系统标识，用于前端决定安装策略
#[tauri::command]
fn get_platform() -> String {
    if cfg!(target_os = "windows") {
        "windows".to_string()
    } else if cfg!(target_os = "macos") {
        "macos".to_string()
    } else {
        "linux".to_string()
    }
}

use tauri::{AppHandle, Emitter, Manager};
use std::io::{Read, Write};
use std::sync::Mutex;

// ============================================================
// PTY 终端状态管理
// ============================================================
struct PtyState {
    writer: Mutex<Option<Box<dyn Write + Send>>>,
    master: Mutex<Option<Box<dyn portable_pty::MasterPty + Send>>>,
}

#[tauri::command]
fn spawn_pty_shell(app: AppHandle, state: tauri::State<'_, PtyState>) -> Result<(), String> {
    let pty_system = portable_pty::native_pty_system();
    let pair = pty_system.openpty(portable_pty::PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    }).map_err(|e| format!("打开 PTY 失败: {}", e))?;

    let mut cmd = if cfg!(target_os = "windows") {
        portable_pty::CommandBuilder::new("powershell.exe")
    } else {
        match std::env::var("SHELL") {
            Ok(shell) => portable_pty::CommandBuilder::new(shell),
            Err(_) => portable_pty::CommandBuilder::new("bash"),
        }
    };

    // 为便携化定制环境变量
    // 1. 设置 npm --prefix 为内置目录
    let portable_root_str = get_portable_dir(app.clone())?;
    let portable_root = PathBuf::from(portable_root_str);
    let claude_bin_dir = portable_root.join("ClaudeBin");
    let node_dir = portable_root.join("Node");
    let git_dir = portable_root.join("Git");
    
    // 我们强制设置 npm_config_prefix，让内部终端调用的 npm 指向便携目录
    cmd.env("npm_config_prefix", claude_bin_dir.to_string_lossy().to_string());

    // 设置 Git Bash 路径 (Claude Code 在 Windows 上需要 bash.exe 来运行底层脚本)
    if cfg!(target_os = "windows") {
        let git_bash = git_dir.join("bin").join("bash.exe");
        if git_bash.exists() {
            cmd.env("CLAUDE_CODE_GIT_BASH_PATH", git_bash.to_string_lossy().to_string());
        }
    }

    // 2. 将 PortableNode 和 ClaudeBin 的 bin 目录注入到 PATH 最前端
    let mut new_paths = vec![];
    
    // Claude 的实际可执行文件位置（由于是本地安装，在 node_modules/.bin 下）
    let claude_actual_bin = if cfg!(target_os = "windows") {
        claude_bin_dir.join("node_modules").join(".bin")
    } else {
        claude_bin_dir.join("node_modules").join(".bin")
    };
    new_paths.push(claude_actual_bin.to_string_lossy().to_string());
    
    // 同时把 ClaudeBin 根目录也放进去（以防万一）
    new_paths.push(claude_bin_dir.to_string_lossy().to_string());

    if cfg!(target_os = "windows") {
        new_paths.push(node_dir.to_string_lossy().to_string());
        new_paths.push(git_dir.join("cmd").to_string_lossy().to_string());
    } else {
        new_paths.push(node_dir.join("bin").to_string_lossy().to_string());
    }
    
    if let Ok(current_path) = std::env::var("PATH") {
        let sep = if cfg!(target_os = "windows") { ";" } else { ":" };
        new_paths.push(current_path);
        cmd.env("PATH", new_paths.join(sep));
    } else {
        let sep = if cfg!(target_os = "windows") { ";" } else { ":" };
        cmd.env("PATH", new_paths.join(sep));
    }
    
    // 根据用户选择的提供商注入 API Key 和 Base URL
    let home_dir = dirs::home_dir().unwrap_or_default();
    let config_path = home_dir.join(".claude").join("config.json");
    if config_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                // 注入 API Key
                if let Some(api_key) = json.get("apiKey").and_then(|v| v.as_str()) {
                    cmd.env("ANTHROPIC_API_KEY", api_key);
                }
                // 根据提供商设置 Base URL
                if let Some(provider) = json.get("provider").and_then(|v| v.as_str()) {
                    if let Some(base_url) = get_base_url_for_provider(provider) {
                        cmd.env("ANTHROPIC_BASE_URL", base_url);
                    }
                }
            }
        }
    }

    let _child = pair.slave.spawn_command(cmd).map_err(|e| format!("启动子进程失败: {}", e))?;

    // 读取 PTY 输出，推送到前端
    let mut reader = pair.master.try_clone_reader().map_err(|e| format!("克隆 reader 失败: {}", e))?;
    let app_clone = app.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 1024];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 { break; }
            let output = String::from_utf8_lossy(&buf[..n]).to_string();
            let _ = app_clone.emit("pty-output", output);
        }
    });

    // 保存 Writer 和 Master 实例
    let writer = pair.master.take_writer().map_err(|e| format!("获取 writer 失败: {}", e))?;
    *state.writer.lock().unwrap() = Some(writer);
    *state.master.lock().unwrap() = Some(pair.master);

    // 当使用第三方提供商时，自动处理 ~/.claude.json：
    //   1. 移除 oauthAccount / primaryApiKey（避免 OAuth 流程劫持）
    //   2. 无条件写入 hasCompletedOnboarding=true（跳过首次引导弹窗）
    // 同时写入 ~/.claude/settings.json 的 apiKeyHelper，作为终极绕过手段
    if config_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(provider) = json.get("provider").and_then(|v| v.as_str()) {
                    if provider != "claude" {
                        // === 处理 ~/.claude.json ===
                        let claude_json_path = home_dir.join(".claude.json");
                        // 若文件不存在则创建最小骨架
                        let mut cj: serde_json::Value = if claude_json_path.exists() {
                            std::fs::read_to_string(&claude_json_path)
                                .ok()
                                .and_then(|s| serde_json::from_str(&s).ok())
                                .unwrap_or_else(|| serde_json::json!({}))
                        } else {
                            serde_json::json!({})
                        };
                        if let Some(obj) = cj.as_object_mut() {
                            obj.remove("oauthAccount");
                            obj.remove("primaryApiKey");
                            // 无条件确保 onboarding 完成（新机器也生效）
                            obj.insert("hasCompletedOnboarding".to_string(), serde_json::json!(true));
                        }
                        if let Ok(new_content) = serde_json::to_string_pretty(&cj) {
                            let _ = std::fs::write(&claude_json_path, new_content);
                            let _ = app.emit("install-log", "[DEBUG] ~/.claude.json 已更新：移除 OAuth 绑定，标记 onboarding 完成".to_string());
                        }

                        // === 写入 ~/.claude/settings.json 的 apiKeyHelper（终极绕过手段）===
                        // Claude Code 会执行此命令获取 Key，完全绕过 OAuth 流程
                        if let Some(api_key) = json.get("apiKey").and_then(|v| v.as_str()) {
                            let settings_dir = home_dir.join(".claude");
                            let _ = std::fs::create_dir_all(&settings_dir);
                            let settings_path = settings_dir.join("settings.json");
                            // 读取已有 settings，避免覆盖其他配置
                            let mut settings: serde_json::Value = if settings_path.exists() {
                                std::fs::read_to_string(&settings_path)
                                    .ok()
                                    .and_then(|s| serde_json::from_str(&s).ok())
                                    .unwrap_or_else(|| serde_json::json!({}))
                            } else {
                                serde_json::json!({})
                            };
                            // apiKeyHelper: Claude Code 执行此 echo 命令来获取 API Key
                            let helper_cmd = if cfg!(target_os = "windows") {
                                format!("cmd /c echo {}", api_key)
                            } else {
                                format!("echo {}", api_key)
                            };
                            if let Some(obj) = settings.as_object_mut() {
                                obj.insert("apiKeyHelper".to_string(), serde_json::json!(helper_cmd));
                            }
                            if let Ok(settings_str) = serde_json::to_string_pretty(&settings) {
                                let _ = std::fs::write(&settings_path, settings_str);
                                let _ = app.emit("install-log", "[DEBUG] ~/.claude/settings.json 已写入 apiKeyHelper".to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    // PTY 启动后，通过 writer 直接写入环境变量设置命令
    // 这比 cmd.env() 更可靠，因为 PowerShell 交互会话有时不继承进程环境变量
    let _ = app.emit("install-log", format!("[DEBUG] 读取配置: {}", config_path.display()));
    if config_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            let _ = app.emit("install-log", format!("[DEBUG] 配置内容: {}", content.trim()));
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                let mut init_cmds = Vec::new();

                // 禁止 Claude Code 发起 OAuth 等非必要网络请求，直接使用 API Key
                if cfg!(target_os = "windows") {
                    init_cmds.push("$env:CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC='1'".to_string());
                } else {
                    init_cmds.push("export CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC='1'".to_string());
                }

                if let Some(api_key) = json.get("apiKey").and_then(|v| v.as_str()) {
                    // 脱敏显示 Key（只显示前6位和后4位）
                    let masked = if api_key.len() > 10 {
                        format!("{}...{}", &api_key[..6], &api_key[api_key.len()-4..])
                    } else {
                        "***".to_string()
                    };
                    let _ = app.emit("install-log", format!("[DEBUG] API Key: {}", masked));
                    if cfg!(target_os = "windows") {
                        init_cmds.push(format!("$env:ANTHROPIC_API_KEY='{}'", api_key));
                    } else {
                        init_cmds.push(format!("export ANTHROPIC_API_KEY='{}'", api_key));
                    }
                } else {
                    let _ = app.emit("install-log", "[DEBUG] ⚠️ 未找到 apiKey 字段！".to_string());
                }

                if let Some(provider) = json.get("provider").and_then(|v| v.as_str()) {
                    let _ = app.emit("install-log", format!("[DEBUG] 提供商: {}", provider));
                    if let Some(base_url) = get_base_url_for_provider(provider) {
                        let _ = app.emit("install-log", format!("[DEBUG] Base URL: {}", base_url));
                        if cfg!(target_os = "windows") {
                            init_cmds.push(format!("$env:ANTHROPIC_BASE_URL='{}'", base_url));
                        } else {
                            init_cmds.push(format!("export ANTHROPIC_BASE_URL='{}'", base_url));
                        }
                    }
                } else {
                    let _ = app.emit("install-log", "[DEBUG] ⚠️ 未找到 provider 字段！".to_string());
                }

                // 注入模型名称（同时设置 ANTHROPIC_DEFAULT_SONNET_MODEL 以兼容内部模型映射）
                if let Some(model_name) = json.get("modelName").and_then(|v| v.as_str()) {
                    if !model_name.is_empty() {
                        let _ = app.emit("install-log", format!("[DEBUG] 模型: {}", model_name));
                        if cfg!(target_os = "windows") {
                            init_cmds.push(format!("$env:ANTHROPIC_MODEL='{}'", model_name));
                            // 某些版本 Claude Code 内部用 sonnet 别名调用，显式映射到国产模型
                            init_cmds.push(format!("$env:ANTHROPIC_DEFAULT_SONNET_MODEL='{}'", model_name));
                        } else {
                            init_cmds.push(format!("export ANTHROPIC_MODEL='{}'", model_name));
                            init_cmds.push(format!("export ANTHROPIC_DEFAULT_SONNET_MODEL='{}'", model_name));
                        }
                    }
                }

                if !init_cmds.is_empty() {
                    // 用分号将所有环境变量命令和 claude 拼成一行执行，确保同一 session 内生效
                    let full_cmd = format!("{}; claude\r\n", init_cmds.join("; "));
                    let _ = app.emit("install-log", format!("[DEBUG] 注入命令: {}", full_cmd));
                    if let Some(ref mut w) = *state.writer.lock().unwrap() {
                        let _ = w.write_all(full_cmd.as_bytes());
                        let _ = w.flush();
                    }
                } else {
                    let _ = app.emit("install-log", "[DEBUG] ⚠️ 没有需要注入的命令！".to_string());
                }
            }
        }
    } else {
        let _ = app.emit("install-log", "[DEBUG] ⚠️ 配置文件不存在！".to_string());
    }

    Ok(())
}

#[tauri::command]
fn write_to_pty(data: String, state: tauri::State<'_, PtyState>) -> Result<(), String> {
    if let Some(writer) = state.writer.lock().unwrap().as_mut() {
        writer.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn resize_pty(rows: u16, cols: u16, state: tauri::State<'_, PtyState>) -> Result<(), String> {
    if let Some(master) = state.master.lock().unwrap().as_mut() {
        master.resize(portable_pty::PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        }).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ============================================================
// 应用入口
// ============================================================
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 注册 State
        .setup(|app| {
            app.manage(PtyState {
                writer: Mutex::new(None),
                master: Mutex::new(None),
            });
            Ok(())
        })
        // 注册 Shell 插件 —— 允许前端执行系统命令
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        // 注册所有 Tauri 命令
        .invoke_handler(tauri::generate_handler![
            get_portable_dir,
            check_portable_node,
            check_global_connectivity,
            check_dependency,
            save_api_key,
            load_api_key,
            check_claude_installed,
            get_platform,
            spawn_pty_shell,
            write_to_pty,
            resize_pty,
            install_claude_portable,
            fetch_models,
        ])
        .run(tauri::generate_context!())
        .expect("启动 Tauri 应用时出错");
}
