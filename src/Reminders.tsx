import { invoke } from "@tauri-apps/api/core";
import { Bell, CalendarDays, RefreshCw } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { EmptyState, GameMark } from "./Accounts";
import { errorMessage, games, type Game } from "./model";

const day = 86_400;
const cacheKey = "game-daily-hub-reminders-v1";
const deadlineFormat = new Intl.DateTimeFormat("zh-CN", {
  month: "short", day: "numeric", hour: "2-digit", minute: "2-digit",
  hourCycle: "h23", timeZone: "Asia/Shanghai",
});
const kindNames = { activity: "活动", pool: "卡池" } as const;
type Kind = keyof typeof kindNames;

interface Reminder {
  title: string;
  startAt: number | null;
  endAt: number;
}

interface Feed {
  game: Game;
  kind: Kind;
  entries: Reminder[];
  error: string | null;
}

interface ReminderEntry extends Reminder {
  game: Game;
  kind: Kind;
}

interface Schedule {
  feeds: Feed[];
  checkedAt: number | null;
}

function readCache(): Schedule {
  try {
    const cached = JSON.parse(localStorage.getItem(cacheKey) ?? "null") as Schedule | null;
    if (cached && Array.isArray(cached.feeds)) return cached;
  } catch {
    // A fresh sync also works when local browser storage is unavailable.
  }
  return { feeds: [], checkedAt: null };
}

function mergeSchedules(previous: Feed[], incoming: Feed[], now: number): Feed[] {
  return incoming.map(feed => {
    const old = previous.find(candidate => candidate.game === feed.game && candidate.kind === feed.kind);
    if (!old || (!feed.error && feed.game !== "reverse1999")) return feed;
    const updatedTitles = new Set(feed.entries.map(entry => entry.title));
    const remembered = old.entries.filter(entry => entry.endAt > now && !updatedTitles.has(entry.title));
    return { ...feed, entries: [...feed.entries, ...remembered] };
  });
}

export function useReminders() {
  const [schedule, setSchedule] = useState(readCache);
  const [now, setNow] = useState(() => Date.now() / 1000);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const refreshing = useRef(false);

  async function refresh() {
    if (refreshing.current) return;
    refreshing.current = true;
    setLoading(true);
    setError("");
    try {
      const incoming = await invoke<Feed[]>("fetch_reminders");
      const checkedAt = Date.now() / 1000;
      setNow(checkedAt);
      setSchedule(previous => ({ feeds: mergeSchedules(previous.feeds, incoming, checkedAt), checkedAt }));
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setLoading(false);
      refreshing.current = false;
    }
  }

  useEffect(() => {
    void refresh();
    const clock = window.setInterval(() => setNow(Date.now() / 1000), 30_000);
    const update = window.setInterval(() => void refresh(), 4 * 60 * 60_000);
    return () => {
      window.clearInterval(clock);
      window.clearInterval(update);
    };
  }, []);

  useEffect(() => {
    try { localStorage.setItem(cacheKey, JSON.stringify(schedule)); } catch { /* cache is optional */ }
  }, [schedule]);

  const entries: ReminderEntry[] = schedule.feeds.flatMap(({ game, kind, entries }) =>
    entries.filter(entry => entry.endAt > now).map(entry => ({ ...entry, game, kind }))
  ).sort((a, b) => a.endAt - b.endAt);
  const expiring = entries.filter(entry => entry.endAt - now <= day && (entry.startAt ?? now) <= now);
  return { entries, expiring, feeds: schedule.feeds, checkedAt: schedule.checkedAt, now, loading, error, refresh };
}

type Reminders = ReturnType<typeof useReminders>;

function remaining(seconds: number) {
  const minutes = Math.ceil(seconds / 60);
  if (seconds > day) {
    return `剩 ${Math.floor(minutes / (24 * 60))} 天 ${Math.floor((minutes % (24 * 60)) / 60)} 小时`;
  }
  if (minutes >= 60) return `剩 ${Math.floor(minutes / 60)} 小时 ${minutes % 60} 分`;
  return `剩 ${minutes} 分钟`;
}

export function ReminderNotice({ entries, open }: { entries: ReminderEntry[]; open: () => void }) {
  if (!entries.length) return null;
  return <div className="notice reminder-notice" role="status">
    <Bell size={17} aria-hidden="true" />
    <div>
      <strong>{entries.length} 项活动或卡池将在 24 小时内结束</strong>
      <p className="small">{entries.slice(0, 2).map(entry => entry.title).join("、")}{entries.length > 2 ? " 等" : ""}</p>
    </div>
    <button className="text-button" type="button" onClick={open}>查看</button>
  </div>;
}

export function RemindersPage({ data }: { data: Reminders }) {
  const [game, setGame] = useState<Game | "all">("all");
  const [kind, setKind] = useState<Kind | "all">("all");
  const { entries, feeds, checkedAt, now, loading, error, refresh } = data;
  const matches = (entry: { game: Game; kind: Kind }) =>
    (game === "all" || entry.game === game) && (kind === "all" || entry.kind === kind);
  const filtered = entries.filter(matches);
  const unavailable = feeds.filter(feed => feed.error && matches(feed));
  const current = filtered.filter(entry => (entry.startAt ?? now) <= now);
  const upcoming = filtered.filter(entry => (entry.startAt ?? now) > now);

  return <>
    <header className="page-header">
      <div>
        <h1>到期提醒</h1>
        <p className="muted">国服 · 北京时间{checkedAt ? ` · ${deadlineFormat.format(checkedAt * 1000)} 检查` : ""}</p>
      </div>
      <button type="button" className="button secondary" onClick={() => void refresh()} disabled={loading}>
        <RefreshCw size={17} className={loading ? "spin" : undefined} />{loading ? "刷新中…" : "刷新日程"}
      </button>
    </header>
    <div className="reminder-filters">
      <select aria-label="筛选提醒游戏" value={game} onChange={event => setGame(event.target.value as Game | "all")}>
        <option value="all">全部游戏</option>
        {Object.entries(games).map(([key, info]) => <option value={key} key={key}>{info.name}</option>)}
      </select>
      <select aria-label="筛选提醒类型" value={kind} onChange={event => setKind(event.target.value as Kind | "all")}>
        <option value="all">活动和卡池</option>
        <option value="activity">只看活动</option>
        <option value="pool">只看卡池</option>
      </select>
    </div>
    {error && <p className="notice error" role="alert">{error}</p>}
    {unavailable.length > 0 && <div className="notice warning" role="status">
      {unavailable.map(feed => <p key={feed.game + feed.kind}>
        {games[feed.game].name} · {kindNames[feed.kind]} · {feed.error}{feed.entries.length > 0 ? " 仍显示已有日程。" : ""}
      </p>)}
    </div>}
    {current.length > 0 && <ReminderList title="到期日程" entries={current} now={now} />}
    {upcoming.length > 0 && <ReminderList title="即将开启" entries={upcoming} now={now} />}
    {!filtered.length && <EmptyState icon={<CalendarDays size={27} />}
      title={loading ? "正在读取日程" : unavailable.length || error ? "暂时无法确认日程" : "暂无已收录日程"} />}
  </>;
}

function ReminderList({ title, entries, now }: { title: string; entries: ReminderEntry[]; now: number }) {
  return <section className="panel reminder-list">
    <div className="panel-title"><h2>{title}</h2></div>
    <ul>{entries.map(entry => {
      const active = (entry.startAt ?? now) <= now;
      const status = active ? remaining(entry.endAt - now)
        : `${deadlineFormat.format((entry.startAt ?? now) * 1000)} 开启`;
      return <li key={`${entry.game}-${entry.kind}-${entry.title}-${entry.endAt}`}>
        <GameMark game={entry.game} />
        <div className="reminder-entry">
          <strong>{entry.title}</strong>
          <p className="small muted">{games[entry.game].name} · {kindNames[entry.kind]}</p>
        </div>
        <div className="reminder-time">
          <span className={"status-pill " + (active && entry.endAt - now <= day ? "warning" : "neutral")}>{status}</span>
          <p className="small muted">{deadlineFormat.format(entry.endAt * 1000)} 截止</p>
        </div>
      </li>;
    })}</ul>
  </section>;
}
