/** Formátování časů a dat (vše v místním čase, v jazyce appky). */

let locale = "cs-CZ";

/** Nastaví App podle zvoleného jazyka před vykreslením. */
export function setFormatLocale(value: string) {
  locale = value;
}

export function stopwatch(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

/** „2 h 05 min“ / „12 min“ / „45 s“ */
export function duration(ms: number): string {
  if (ms > 0 && ms < 60000) return `${Math.floor(ms / 1000)} s`;
  const minutes = Math.floor(Math.max(0, ms) / 60000);
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  if (h === 0) return `${m} min`;
  return `${h} h ${String(m).padStart(2, "0")} min`;
}

export function clock(ms: number): string {
  return new Date(ms).toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" });
}

export function clockWithSeconds(ms: number): string {
  return new Date(ms).toLocaleTimeString(locale);
}

export function longDate(d: Date): string {
  return d.toLocaleDateString(locale, {
    weekday: "long",
    day: "numeric",
    month: "long",
    year: "numeric",
  });
}

export function shortDate(iso: string): string {
  return parseIso(iso).toLocaleDateString(locale, {
    weekday: "short",
    day: "numeric",
    month: "numeric",
  });
}

/** Datum → „2026-09-26“ v místním čase. */
export function toIso(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

export function parseIso(iso: string): Date {
  const [y, m, d] = iso.split("-").map(Number);
  return new Date(y, m - 1, d);
}

export type Period = "day" | "week" | "month";

export function periodRange(period: Period, anchor: Date): { from: Date; to: Date } {
  const a = new Date(anchor.getFullYear(), anchor.getMonth(), anchor.getDate());
  if (period === "day") return { from: a, to: a };
  if (period === "week") {
    const mondayOffset = (a.getDay() + 6) % 7;
    const from = new Date(a.getFullYear(), a.getMonth(), a.getDate() - mondayOffset);
    const to = new Date(from.getFullYear(), from.getMonth(), from.getDate() + 6);
    return { from, to };
  }
  return {
    from: new Date(a.getFullYear(), a.getMonth(), 1),
    to: new Date(a.getFullYear(), a.getMonth() + 1, 0),
  };
}

export function shiftAnchor(period: Period, anchor: Date, step: number): Date {
  const a = new Date(anchor);
  if (period === "day") a.setDate(a.getDate() + step);
  else if (period === "week") a.setDate(a.getDate() + 7 * step);
  else a.setMonth(a.getMonth() + step, 1);
  return a;
}

export function periodLabel(period: Period, anchor: Date): string {
  const { from, to } = periodRange(period, anchor);
  if (period === "day") return longDate(from);
  if (period === "month") {
    return from.toLocaleDateString(locale, { month: "long", year: "numeric" });
  }
  const f = from.toLocaleDateString(locale, { day: "numeric", month: "numeric" });
  const t = to.toLocaleDateString(locale, { day: "numeric", month: "numeric", year: "numeric" });
  return `${f} – ${t}`;
}
