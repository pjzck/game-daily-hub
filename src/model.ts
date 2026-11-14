export const games = {
  genshin: { name: "原神", mark: "原", stamina: "原粹树脂", activity: "每日委托" },
  starrail: { name: "崩坏：星穹铁道", mark: "铁", stamina: "开拓力", activity: "每日实训" },
  arknights: { name: "明日方舟", mark: "方", stamina: "理智", activity: "每日任务" },
  reverse1999: { name: "重返未来：1999", mark: "返", stamina: "活性", activity: "每日任务" },
} as const;

export type Game = keyof typeof games;

export interface Meter {
  current: number;
  maximum: number;
}

export interface Role {
  game: Game;
  name: string;
  uid: string;
  server: string;
  serverName: string;
  level: number | null;
}

export interface Account {
  id: string;
  role: Role;
  source: "manual" | "miyoushe";
  followed: boolean;
  connection: string | null;
  profileAt: number;
  notesAt: number | null;
  notes: {
    stamina: Meter | null;
    activity: Meter | null;
    fullAt: number | null;
    assignments: Meter | null;
    assignmentsReady: number | null;
    homeCoin: Meter | null;
    reserveStamina: number | null;
    reserveFull: boolean | null;
    weeklyActivity: Meter | null;
    simulatedUniverse: Meter | null;
  } | null;
  issue: string | null;
}

export const dateTime = new Intl.DateTimeFormat("zh-CN", {
  month: "short",
  day: "numeric",
  hour: "2-digit",
  minute: "2-digit",
  hourCycle: "h23",
});

export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "操作失败，请重试。";
}
