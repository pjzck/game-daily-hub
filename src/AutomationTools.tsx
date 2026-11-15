import { invoke } from "@tauri-apps/api/core";
import { Gamepad2, Play, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { errorMessage, games } from "./model";

const tools = [
  { id: "genshin", name: games.genshin.name, subtitle: "PC 客户端" },
  { id: "starrail", name: games.starrail.name, subtitle: "PC 客户端" },
  { id: "maa", name: "MAA", subtitle: games.arknights.name },
  { id: "m9a", name: "M9A", subtitle: games.reverse1999.name },
] as const;

type Tool = typeof tools[number]["id"] | "mumu";
type Action = "launch" | "choose" | "reset";
const commands: Record<Action, string> = {
  launch: "launch_automation_tool",
  choose: "choose_launch_path",
  reset: "reset_launch_path",
};
interface ToolStatus {
  tool: Tool;
  status: "ready" | "running" | "missing";
  path: string | null;
  source: "auto" | "manual" | "environment";
}

export function useLaunchers() {
  const [rows, setRows] = useState<ToolStatus[]>([]);
  const [loading, setLoading] = useState(true);
  const [pending, setPending] = useState<Partial<Record<Tool, Action>>>({});
  const [issues, setIssues] = useState<Partial<Record<Tool, string>>>({});
  const [error, setError] = useState("");

  async function refresh() {
    setLoading(true);
    setError("");
    try {
      setRows(await invoke<ToolStatus[]>("list_automation_tools"));
      setIssues({});
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function run(tool: Tool, action: Action) {
    setPending(current => ({ ...current, [tool]: action }));
    setIssues(current => ({ ...current, [tool]: undefined }));
    try {
      const result = await invoke<ToolStatus | null>(commands[action], { tool });
      if (!result) return false;
      setRows(current => [...current.filter(row => row.tool !== tool), result]);
      return true;
    } catch (reason) {
      setIssues(current => ({ ...current, [tool]: errorMessage(reason) }));
      return false;
    } finally {
      setPending(current => ({ ...current, [tool]: undefined }));
    }
  }

  useEffect(() => { void refresh(); }, []);

  return { rows, loading, pending, issues, error, refresh, run };
}

type Launchers = ReturnType<typeof useLaunchers>;

export function LauncherPath({ row }: { row?: ToolStatus }) {
  if (!row) return null;
  const source = row.source === "manual" ? "手动指定" : row.source === "environment" ? "环境变量" : "自动检测";
  return <p className="small muted launcher-path" title={row.path ?? undefined}>
    {source} · {row.path ?? "未找到启动文件"}
  </p>;
}

export function LauncherPathActions({ row, disabled, onChoose, onReset }: {
  row?: ToolStatus; disabled: boolean; onChoose: () => void; onReset: () => void;
}) {
  return <>
    <button type="button" className="button secondary device-start" disabled={disabled} onClick={onChoose}>
      {row?.source === "manual" ? "更换路径" : "手动添加"}
    </button>
    {row?.source === "manual" && <button type="button" className="button secondary device-start" disabled={disabled} onClick={onReset}>
      恢复自动
    </button>}
  </>;
}

export function AutomationTools({ data }: { data: Launchers }) {
  const { rows, loading, pending, issues, error, refresh, run } = data;
  return <section className="panel automation-tools" aria-labelledby="automation-tools-title">
    <div className="panel-title">
      <h2 id="automation-tools-title">游戏与辅助工具</h2>
      <button type="button" className="button secondary device-start" onClick={() => void refresh()}
        disabled={loading || Object.values(pending).some(Boolean)}>
        <RefreshCw size={15} className={loading ? "spin" : undefined} />自动检测
      </button>
    </div>
    {error && <p className="notice error" role="alert">{error}</p>}
    <ul className="automation-tool-list">
      {tools.map(tool => {
        const row = rows.find(row => row.tool === tool.id);
        const status = row?.status;
        const action = pending[tool.id];
        const failure = issues[tool.id];
        return <li key={tool.id}>
          <div className="table-device">
            <span className="device-mark"><Gamepad2 size={18} aria-hidden="true" /></span>
            <div className="launcher-label"><strong>{tool.name}</strong><p className="small muted">{tool.subtitle}</p>
              <LauncherPath row={row} />
            </div>
          </div>
          <div className="automation-tool-actions">
            <span className={"status-pill " + (status === "running" ? "good" : "neutral")} role="status">
              {action === "launch" ? "启动中" : status === "running" ? "运行中" : status === "ready" ? "可启动"
                : status === "missing" ? "未找到" : loading ? "读取中" : "未知"}
            </span>
            <button type="button" className="button secondary device-start" aria-label={`启动 ${tool.name}`}
              disabled={loading || !!action || status !== "ready"} onClick={() => void run(tool.id, "launch")}>
              <Play size={15} />启动
            </button>
            <LauncherPathActions row={row} disabled={loading || !!action}
              onChoose={() => void run(tool.id, "choose")} onReset={() => void run(tool.id, "reset")} />
          </div>
          {failure && <p className="small device-start-error automation-tool-note" role="alert">{failure}</p>}
        </li>;
      })}
    </ul>
  </section>;
}
