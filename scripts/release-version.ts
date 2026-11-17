import { $ } from "bun";

const calver = /^(\d{4}\.\d{2}\.\d{2})-(0|[1-9]\d*)$/;
const sydneyDate = new Intl.DateTimeFormat("en-CA", {
  timeZone: "Australia/Sydney",
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
});

export function releaseVersion(
  now: Date,
  tags: string[],
  tagsAtCommit: string[],
): string {
  // Retries keep their original version, even when they cross midnight.
  const existing = tagsAtCommit.filter((tag) => calver.test(tag));
  if (existing.length > 1)
    throw new Error("Multiple CalVer tags point at this commit");
  if (existing.length === 1) return existing[0]!;

  const day = sydneyDate.format(now).replaceAll("-", ".");
  let next = 0;
  for (const tag of tags) {
    const match = calver.exec(tag);
    if (match?.[1] === day) next = Math.max(next, Number(match[2]) + 1);
  }
  if (next > 65535)
    throw new Error("Build number exceeds a Windows version component");
  return `${day}-${next}`;
}

if (import.meta.main) {
  const tags = (await $`git tag --list`.text()).trim().split(/\r?\n/);
  const atCommit = (await $`git tag --points-at HEAD`.text())
    .trim()
    .split(/\r?\n/);
  console.log(releaseVersion(new Date(), tags, atCommit));
}
