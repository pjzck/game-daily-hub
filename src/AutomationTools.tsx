import { invoke } from "@tauri-apps/api/core";
import { Gamepad2, Play, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { errorMessage } from "./model";

const tools = [
  { id: "maa", name: "MAA", game: "明日方舟" },
  { id: "m9a", name: "M9A", game: "重返未来：1999" },
] as const;

type Tool = typeof tools[number]["id"];
interface ToolStatus {
  tool: Tool;
  status: "ready" | "running" | "missing";
}

export function AutomationTools() {
  const [rows, setRows] = useState<ToolStatus[]>([]);
  const [loading, setLoading] = useState(true);
  const [launching, setLaunching] = useState<Partial<Record<Tool, boolean>>>({});
  const [launchErrors, setLaunchErrors] = useState<Partial<Record<Tool, string>>>({});
  const [error, setError] = useState("");

  async function refresh() {
    setLoading(true);
    setError("");
    try {
      setRows(await invoke<ToolStatus[]>("list_automation_tools"));
      setLaunchErrors({});
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function launch(tool: Tool) {
    setLaunching(current => ({ ...current, [tool]: true }));
    setLaunchErrors(current => ({ ...current, [tool]: undefined }));
    try {
      const result = await invoke<ToolStatus>("launch_automation_tool", { tool });
      setRows(current => current.map(row => row.tool === tool ? result : row));
    } catch (reason) {
      setLaunchErrors(current => ({ ...current, [tool]: errorMessage(reason) }));
    } finally {
      setLaunching(current => ({ ...current, [tool]: false }));
    }
  }

  useEffect(() => { void refresh(); }, []);

  return <section className="panel automation-tools" aria-labelledby="automation-tools-title">
    <div className="panel-title">
      <h2 id="automation-tools-title">辅助工具</h2>
      <button type="button" className="button secondary device-start" onClick={() => void refresh()}
        disabled={loading || Object.values(launching).some(Boolean)}>
        <RefreshCw size={15} className={loading ? "spin" : undefined} />刷新工具
      </button>
    </div>
    {error && <p className="notice error" role="alert">{error}</p>}
    <ul className="automation-tool-list">
      {tools.map(tool => {
        const status = rows.find(row => row.tool === tool.id)?.status;
        const pending = launching[tool.id];
        const failure = launchErrors[tool.id];
        return <li key={tool.id}>
          <div className="table-device">
            <span className="device-mark"><Gamepad2 size={18} aria-hidden="true" /></span>
            <div><strong>{tool.name}</strong><p className="small muted">{tool.game}</p></div>
          </div>
          <div className="automation-tool-actions">
            <span className={"status-pill " + (status === "running" ? "good" : "neutral")} role="status">
              {pending ? "启动中" : status === "running" ? "运行中" : status === "ready" ? "可启动"
                : status === "missing" ? "未找到" : loading ? "读取中" : "未知"}
            </span>
            <button type="button" className="button secondary device-start" aria-label={`启动 ${tool.name}`}
              disabled={loading || pending || status !== "ready"} onClick={() => void launch(tool.id)}>
              <Play size={15} />启动
            </button>
          </div>
          {failure && <p className="small device-start-error automation-tool-note" role="alert">{failure}</p>}
          {status === "missing" && <p className="small muted automation-tool-note">添加 {tool.name} 的桌面快捷方式后刷新。</p>}
        </li>;
      })}
    </ul>
  </section>;
}
