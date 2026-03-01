# Tauri + React + Typescript

This template should help get you started developing with Tauri, React and Typescript in Vite.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)


# HuiCopilotCode

这是基于 Tauri + React + TypeScript 构建的桌面应用程序项目。

## 项目结构

```
src-tauri/          # Tauri Rust 后端
├── src/
│   ├── main.rs     # 主入口
│   ├── lib.rs      # 业务逻辑
│   └── commands.rs # Tauri 命令
├── Cargo.toml      # Rust 依赖管理
└── gen/            # 自动生成的代码

src/                # React 前端
├── main.tsx        # React 入口
├── App.tsx         # 主组件
├── components/     # UI 组件
└── services/       # API 服务

.env                # 环境变量


## 运行开发服务器

### 前端
```bash
npm run dev
```

### 后端
```bash
$env:Path = "$env:USERPROFILE\.cargo\bin;" + $env:Path; npm run tauri dev

```

## 构建

```bash
$env:Path = "$env:USERPROFILE\.cargo\bin;" + $env:Path; npm run tauri build
```

## 环境变量

创建 `.env` 文件并添加以下变量：

```env
CLAUDE_API_KEY=your_claude_api_key
ZHIPU_API_KEY=your_zhipu_api_key
```
