# HuiCopilotCode 踩坑记录

> 记录开发过程中遇到的问题及解决方案，避免重复踩坑。

---

## 坑 1：`npm run tauri dev/build` 报 `cargo not found`

### 现象

```
failed to run 'cargo metadata' command to get workspace directory:
failed to run command cargo metadata --no-deps --format-version 1: program not found
```

### 原因

Rust/Cargo 已安装（路径为 `C:\Users\Administrator\.cargo\bin\`），但 IDE 集成终端（VS Code / Cursor 等）启动时**不完整继承用户 PATH**，导致 `cargo` 在该终端 session 中不可见。

### 解决方案

**方案 A（推荐）：在终端内临时追加 PATH**，适用于当前 session：

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;" + $env:PATH
npm run tauri dev
```

**方案 B：写入 PowerShell Profile**，让每次新开终端自动生效：

```powershell
# 写入 Profile（只需执行一次）
Add-Content -Path $PROFILE -Value '$env:PATH = "$env:USERPROFILE\.cargo\bin;" + $env:PATH'
```

Profile 路径：`D:\OneDrive\文档\WindowsPowerShell\Microsoft.PowerShell_profile.ps1`

> ⚠️ 注意：写入 Profile 后需**重新打开终端**才生效；在 IDE 内置终端中如仍不生效，请关闭并重新打开 IDE。

---

## 坑 2：启动交互终端时环境变量硬编码

### 现象

`EmbeddedTerminal.tsx` 中 PTY 启动后用 `write_to_pty` 硬编码写入了固定的 API Key 和 Model，导致用户在界面上修改配置后终端仍然使用旧值。

### 原因

前端直接拼接了写死的环境变量命令，没有读取用户保存的配置。

### 解决方案

后端 `spawn_pty_shell`（`lib.rs`）已自动读取 `~/.claude/config.json` 动态注入环境变量并启动 `claude`，**前端无需也不应再重复发送**。删除 `EmbeddedTerminal.tsx` 中多余的 `write_to_pty`  调用即可。

---

## 坑 3：`claude` 终端环境变量在同一 session 内需一行完成

### 现象

在终端中先执行环境变量设置，再执行 `claude`，如果分两个命令或两个终端 session 执行，环境变量会丢失。

### 解决方案

在同一 session 中用 `;` 连接，PowerShell 中可用反引号 `` ` `` 换行（纯视觉换行，本质仍是同一条命令）：

```powershell
$env:CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC='1'; `
$env:ANTHROPIC_API_KEY='<KEY>'; `
$env:ANTHROPIC_BASE_URL='https://open.bigmodel.cn/api/anthropic'; `
$env:ANTHROPIC_MODEL='glm-4.5'; `
claude
```

> 反引号 `` ` `` 是 PowerShell 的行继续符，等价于把上面五行写成一行，确保所有变量在同一进程 session 中生效后再启动 `claude`。

---

## 坑 4：Rust 编译 warning：`unused import: tauri::Manager`

### 现象

```rust
warning: unused import: `tauri::Manager`
   --> src\lib.rs:191:9
    |
191 |     use tauri::Manager;
    |         ^^^^^^^^^^^^^^
```

### 原因

`install_claude_portable` 函数内定义了一个 `use tauri::Manager;`，但该函数在实现中并未发现使用 `Manager` trait 的任何方法，导致编译器报警。

### 解决方案

删除 `lib.rs` 中对应的 `use tauri::Manager;` 语句即可消除 warning。
```

