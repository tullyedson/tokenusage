import type { UsageMeter } from "./types";

export function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, char => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[char] ?? char));
}
export function percent(value: number | null): string {
  return value === null || !Number.isFinite(value) ? "Unavailable" : `${new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 }).format(Math.max(0, Math.min(100, value)))}% left`;
}
export function quantity(value: number, unit: string): string {
  if (unit === "USD") return new Intl.NumberFormat(undefined, { style: "currency", currency: "USD", maximumFractionDigits: 2 }).format(value);
  return `${new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 }).format(value)} ${unit}`;
}
export function balance(meter: UsageMeter): string {
  if (meter.remaining === null) return "";
  return `${quantity(meter.remaining, meter.unit)} left${meter.limit === null ? "" : ` of ${quantity(meter.limit, meter.unit)}`}`;
}
export function resetLabel(timestamp: number | null, now = Date.now()): string {
  if (timestamp === null) return "";
  if (timestamp * 1000 <= now) return "Reset time passed. Refresh to confirm.";
  return `Resets ${new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" }).format(timestamp * 1000)}`;
}
export function updatedLabel(timestamp: number | null): string {
  if (timestamp === null) return "Waiting for first reading";
  return `Updated ${new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" }).format(timestamp * 1000)}`;
}
export function meterTone(meter: UsageMeter): string {
  return meter.percentLeft === null ? "unknown" : meter.percentLeft <= 10 ? "low" : meter.percentLeft <= 25 ? "watch" : "healthy";
}
