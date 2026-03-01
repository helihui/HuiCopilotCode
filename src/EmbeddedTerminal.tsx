import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { X, Maximize2, Minimize2 } from "lucide-react";
import "@xterm/xterm/css/xterm.css";

interface EmbeddedTerminalProps {
  onClose: () => void;
}

export function EmbeddedTerminal({ onClose }: EmbeddedTerminalProps) {
  const terminalRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const [isFullscreen, setIsFullscreen] = useState(false);

  useEffect(() => {
    if (!terminalRef.current) return;

    // Initialize xterm.js
    const term = new Terminal({
      theme: {
        background: "#0f111a",
        foreground: "#e4e4e7",
        cursor: "#a78bfa",
        selectionBackground: "rgba(167, 139, 250, 0.3)",
      },
      fontFamily: "var(--font-mono, ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace)",
      fontSize: 14,
      cursorBlink: true,
      scrollback: 5000,
    });
    xtermRef.current = term;

    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    fitAddonRef.current = fitAddon;

    term.open(terminalRef.current);
    fitAddon.fit();

    let unlisten: UnlistenFn | null = null;

    const setupPty = async () => {
      // 监听 PTY 输出
      unlisten = await listen<string>("pty-output", (event) => {
        term.write(event.payload);
      });

      // 发送输入到 PTY
      term.onData((data) => {
        invoke("write_to_pty", { data }).catch(console.error);
      });

      // 处理调整大小
      const handleResize = () => {
        if (fitAddonRef.current && xtermRef.current) {
          try {
            fitAddonRef.current.fit();
            const { rows, cols } = xtermRef.current;
            invoke("resize_pty", { rows, cols }).catch(console.error);
          } catch (e) {
            console.error("Fit error:", e);
          }
        }
      };

      term.onResize(({ cols, rows }) => {
        invoke("resize_pty", { rows, cols }).catch(console.error);
      });

      window.addEventListener("resize", handleResize);

      // 启动 PTY Shell
      try {
        await invoke("spawn_pty_shell");
        term.write('\x1b[32mPTY 连接成功。终端已就绪。\x1b[0m\r\n\r\n');
        // 自动输入 claude 并回车
        invoke("write_to_pty", { data: "claude\r" }).catch(console.error);
        handleResize();
      } catch (err) {
        term.write(`\r\n\x1b[31m启动终端失败: ${err}\x1b[0m\r\n`);
      }

      return () => {
        window.removeEventListener("resize", handleResize);
        if (unlisten) unlisten();
        term.dispose();
      };
    };

    const cleanupPromise = setupPty();

    return () => {
      cleanupPromise.then(cleanup => cleanup && cleanup());
    };
  }, []);

  // 监听全屏状态改变以重新调整终端大小
  useEffect(() => {
    const timer = setTimeout(() => {
      if (fitAddonRef.current && xtermRef.current) {
         try {
           fitAddonRef.current.fit();
           const { rows, cols } = xtermRef.current;
           invoke("resize_pty", { rows, cols }).catch(console.error);
         } catch(e) {}
      }
    }, 100);
    return () => clearTimeout(timer);
  }, [isFullscreen]);

  return (
    <div
      className={`flex flex-col bg-[#0f111a] border border-border shadow-2xl transition-all duration-300 z-50 ${
        isFullscreen
          ? "fixed inset-0"
          : "absolute bottom-0 right-6 w-[800px] h-[550px] rounded-t-xl"
      }`}
    >
      {/* 终端标题栏 */}
      <div className="flex items-center justify-between px-4 py-3 bg-bg-secondary border-b border-border select-none rounded-t-xl">
        <div className="flex items-center gap-2">
          <span className="w-3 h-3 rounded-full bg-error" />
          <span className="w-3 h-3 rounded-full bg-warning" />
          <span className="w-3 h-3 rounded-full bg-success" />
          <span className="ml-2 text-xs font-mono font-medium text-text-secondary">
            Claude Code 交互终端
          </span>
        </div>
        <div className="flex items-center gap-1">
          <button
            onClick={() => setIsFullscreen(!isFullscreen)}
            className="p-1.5 text-text-muted hover:text-text-primary hover:bg-white/5 rounded transition-colors cursor-pointer"
            title={isFullscreen ? "退出全屏" : "全屏"}
          >
            {isFullscreen ? <Minimize2 className="w-4 h-4" /> : <Maximize2 className="w-4 h-4" />}
          </button>
          <button
            onClick={onClose}
            className="p-1.5 text-text-muted hover:text-error hover:bg-error/10 rounded transition-colors cursor-pointer"
            title="关闭终端"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
      </div>

      {/* xterm.js 容器 */}
      <div className="flex-1 p-3 overflow-hidden" ref={terminalRef} />
    </div>
  );
}
