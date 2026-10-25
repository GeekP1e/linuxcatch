import { forwardRef, useEffect, useImperativeHandle, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import type {
  CheckResult,
  ConnectionStatus,
  Language,
  LocalText,
} from "./types";
import { message, ui } from "./i18n";

export type TerminalHandle = { check: () => boolean; focus: () => void };

type Props = {
  lessonId: string;
  language: Language;
  onStatus: (status: ConnectionStatus) => void;
  onResult: (result: CheckResult) => void;
  onError: (message: LocalText) => void;
};

export const TerminalPane = forwardRef<TerminalHandle, Props>(
  function TerminalPane(
    { lessonId, language, onStatus, onResult, onError },
    ref,
  ) {
    const languageRef = useRef(language);
    languageRef.current = language;
    const host = useRef<HTMLDivElement>(null);
    const socket = useRef<WebSocket | null>(null);
    const terminal = useRef<Terminal | null>(null);
    const ready = useRef(false);

    useImperativeHandle(
      ref,
      () => ({
        check: () => {
          if (!ready.current || socket.current?.readyState !== WebSocket.OPEN)
            return false;
          socket.current.send(JSON.stringify({ type: "check" }));
          return true;
        },
        focus: () => terminal.current?.focus(),
      }),
      [],
    );

    useEffect(() => {
      if (!host.current) return;
      let disposed = false;
      ready.current = false;
      onStatus("connecting");
      const term = new Terminal({
        cursorBlink: true,
        cursorStyle: "block",
        fontFamily:
          '"Fira Code", Consolas, Monaco, "Liberation Mono", monospace',
        fontSize: 14,
        lineHeight: 1.45,
        scrollback: 3000,
        theme: {
          background: "#050806",
          foreground: "#e5e7eb",
          cursor: "#4ade80",
          cursorAccent: "#031107",
          selectionBackground: "#164829",
          green: "#4ade80",
          blue: "#86a991",
        },
      });
      const fit = new FitAddon();
      term.loadAddon(fit);
      term.open(host.current);
      term.attachCustomKeyEventHandler((event) => {
        if (
          event.type === "keydown" &&
          event.ctrlKey &&
          ["w", "t", "n", "p"].includes(event.key.toLowerCase())
        ) {
          event.preventDefault();
        }
        return true;
      });
      fit.fit();
      terminal.current = term;
      term.writeln(
        `\x1b[38;5;114mLinuxCatch\x1b[0m · ${ui[languageRef.current].welcome}`,
      );
      term.writeln(
        `\x1b[90m${ui[languageRef.current].openingEnvironment}\x1b[0m\r\n`,
      );
      const protocol = window.location.protocol === "https:" ? "wss" : "ws";
      const ws = new WebSocket(
        `${protocol}://${window.location.host}/api/terminal/${encodeURIComponent(lessonId)}`,
      );
      ws.binaryType = "arraybuffer";
      socket.current = ws;
      const resize = () => {
        if (disposed) return;
        fit.fit();
        if (ready.current && ws.readyState === WebSocket.OPEN) {
          ws.send(
            JSON.stringify({
              type: "resize",
              cols: term.cols,
              rows: term.rows,
            }),
          );
        }
      };
      const observer = new ResizeObserver(resize);
      observer.observe(host.current);
      const input = term.onData((data) => {
        if (ready.current && ws.readyState === WebSocket.OPEN) {
          const bytes = new TextEncoder().encode(data);
          // The server accepts WebSocket messages up to 16 KiB, including pastes.
          for (let offset = 0; offset < bytes.length; offset += 16 * 1024) {
            ws.send(bytes.subarray(offset, offset + 16 * 1024));
          }
        }
      });
      const openingTimer = window.setTimeout(() => {
        if (!ready.current && !disposed) {
          disposed = true;
          onStatus("error");
          onError(message("openingTimeout"));
          ws.close();
        }
      }, 30_000);
      ws.onmessage = (event: MessageEvent<string | ArrayBuffer>) => {
        if (disposed) return;
        if (event.data instanceof ArrayBuffer) {
          term.write(new Uint8Array(event.data));
          return;
        }
        try {
          const message = JSON.parse(event.data);
          if (message.type === "ready") {
            window.clearTimeout(openingTimer);
            ready.current = true;
            onStatus("ready");
            term.clear();
            resize();
            term.focus();
          } else if (message.type === "checked") {
            onResult({ passed: message.passed, message: message.message });
          } else if (message.type === "error" || message.type === "expired") {
            ready.current = false;
            window.clearTimeout(openingTimer);
            onStatus("error");
            onError(message.message);
            term.writeln(
              `\r\n\x1b[33m${message.message[languageRef.current]}\x1b[0m`,
            );
          }
        } catch {
          onError(message("responseError"));
        }
      };
      ws.onerror = () => {
        if (disposed) return;
        ready.current = false;
        window.clearTimeout(openingTimer);
        onStatus("error");
        onError(message("connectionError"));
      };
      ws.onclose = () => {
        if (disposed) return;
        ready.current = false;
        window.clearTimeout(openingTimer);
        onStatus("closed");
      };
      return () => {
        disposed = true;
        ready.current = false;
        window.clearTimeout(openingTimer);
        observer.disconnect();
        input.dispose();
        ws.close();
        term.dispose();
        socket.current = null;
        terminal.current = null;
      };
    }, [lessonId, onStatus, onResult, onError]);

    return (
      <div
        className="terminal-host"
        role="region"
        aria-label={ui[language].terminal}
      >
        <div className="terminal-mount" ref={host} />
      </div>
    );
  },
);
