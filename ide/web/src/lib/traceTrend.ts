import type { DebugTrace } from "@/types";

export type TrendPoint = {
  cycle: number;
  value: string | number | boolean;
  at: string;
};

export type TrendSeries = {
  name: string;
  points: TrendPoint[];
};

const MAX_TREND_POINTS = 500;

export function recordTrendSeries(
  existing: TrendSeries[],
  trace: DebugTrace,
  watchNames: string[]
): TrendSeries[] {
  const names = watchNames.length > 0 ? watchNames : trace.cycles.at(-1)?.watches.map((w) => w.name) ?? [];
  const byName = new Map(existing.map((series) => [series.name, [...series.points]]));

  for (const cycle of trace.cycles) {
    const at = new Date().toISOString();
    for (const name of names) {
      const watch = cycle.watches.find((entry) => entry.name === name);
      const variable = cycle.variables.find((entry) => entry.name === name);
      const value = watch?.value ?? variable?.value;
      if (value === undefined) {
        continue;
      }
      const bucket = byName.get(name) ?? [];
      bucket.push({ cycle: cycle.cycle, value, at });
      byName.set(name, bucket.slice(-MAX_TREND_POINTS));
    }
  }

  return [...byName.entries()]
    .map(([name, points]) => ({ name, points }))
    .sort((a, b) => a.name.localeCompare(b.name));
}

export function trendSparkline(points: TrendPoint[], width = 120, height = 24): string {
  if (points.length < 2) {
    return "";
  }
  const numeric = points.map((point) => Number(point.value)).filter((value) => !Number.isNaN(value));
  if (numeric.length < 2) {
    return "";
  }
  const min = Math.min(...numeric);
  const max = Math.max(...numeric);
  const range = max - min || 1;
  const step = width / (numeric.length - 1);
  const coords = numeric
    .map((value, index) => {
      const x = index * step;
      const y = height - ((value - min) / range) * height;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
  return coords;
}
