import { expect, test } from "bun:test";
import { releaseVersion } from "./release-version";

test("Sydney midnight starts a new counter in summer and winter", () => {
  const cases = [
    ["2026-11-17T12:59:59Z", "2026.11.17-0"],
    ["2026-11-17T13:00:00Z", "2026.11.18-0"],
    ["2026-07-17T13:59:59Z", "2026.07.17-0"],
    ["2026-07-17T14:00:00Z", "2026.07.18-0"],
  ] as const;
  for (const [instant, tag] of cases) {
    expect(releaseVersion(new Date(instant), [], [])).toBe(tag);
  }
});

test("increment the highest numeric suffix among the day's CalVer tags", () => {
  const tags = [
    "2026.11.17-20",
    "2026.11.18-9",
    "2026.11.18-11",
    "2026.11.18-rc",
    "v2",
  ];
  expect(releaseVersion(new Date("2026-11-18T00:00Z"), tags, [])).toBe(
    "2026.11.18-12",
  );
});

test("reruns reuse the exact commit's existing tag across days", () => {
  const now = new Date("2026-11-18T00:00Z");
  expect(releaseVersion(now, [], ["v2", "2026.11.17-3"])).toBe("2026.11.17-3");
  expect(() =>
    releaseVersion(now, [], ["2026.11.17-3", "2026.11.18-1"]),
  ).toThrow();
});
