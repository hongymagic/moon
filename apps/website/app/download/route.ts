const releases = "https://github.com/hongymagic/moon/releases";
const latestReleaseApi =
  "https://api.github.com/repos/hongymagic/moon/releases/latest";

type Release = {
  tag_name?: unknown;
  draft?: unknown;
  prerelease?: unknown;
  assets?: unknown;
};

async function windowsDownload(zipOnly: boolean): Promise<string | undefined> {
  const response = await fetch(latestReleaseApi, {
    headers: {
      Accept: "application/vnd.github+json",
      "User-Agent": "Moon-Website",
      "X-GitHub-Api-Version": "2026-03-10",
    },
    signal: AbortSignal.timeout(4000),
    next: { revalidate: 3600 },
  });
  if (!response.ok) return;

  const release: Release | null = await response.json();
  const tag = release?.tag_name;
  if (
    typeof tag !== "string" ||
    !/^\d{4}\.\d{2}\.\d{2}-(0|[1-9]\d*)$/.test(tag) ||
    release?.draft !== false ||
    release.prerelease !== false ||
    !Array.isArray(release.assets)
  ) {
    return;
  }

  const suffixes = zipOnly ? [".zip"] : ["-setup.exe", ".zip"];
  for (const suffix of suffixes) {
    const name = `moon-${tag}-windows-x64${suffix}`;
    const url = `${releases}/download/${tag}/${name}`;
    if (
      release.assets.some(
        (asset) =>
          asset?.state === "uploaded" &&
          asset.name === name &&
          asset.browser_download_url === url,
      )
    ) {
      return url;
    }
  }
}

export async function GET(request: Request): Promise<Response> {
  const zipOnly = new URL(request.url).searchParams.get("format") === "zip";
  const download = await windowsDownload(zipOnly).catch(() => undefined);
  return new Response(null, {
    status: 307,
    headers: {
      Location: download ?? `${releases}/latest`,
      "Cache-Control": "no-store",
    },
  });
}
