const latestRelease = "https://github.com/hongymagic/moon/releases/latest";

export function GET(request: Request): Response {
  const { searchParams } = new URL(request.url);
  const architecture = searchParams.get("arch") ?? "x64";
  const suffix = searchParams.get("format") === "zip" ? ".zip" : "-setup.exe";
  const location =
    architecture === "x64" || architecture === "arm64"
      ? `${latestRelease}/download/moon-windows-${architecture}${suffix}`
      : latestRelease;

  return new Response(null, {
    status: 307,
    headers: { Location: location, "Cache-Control": "no-store" },
  });
}
