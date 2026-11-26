// Pass the output of `gh release view --json tagName,assets` on stdin.
const release: { tagName: string; assets: { name: string; digest: string }[] } =
  await Bun.stdin.json();
const website = Bun.argv[2] ?? "https://moon.hongy.dev";
const latest = "https://github.com/hongymagic/moon/releases/latest/download";

const homepage = await fetch(website, { signal: AbortSignal.timeout(20_000) });
if (!homepage.ok) throw new Error(`${website} returned HTTP ${homepage.status}`);
const links = new Set<string>();
await new HTMLRewriter()
  .on("a[href]", {
    element(anchor) {
      const href = anchor.getAttribute("href")?.replaceAll("&amp;", "&");
      if (href) links.add(new URL(href, homepage.url).href);
    },
  })
  .transform(homepage)
  .text();

for (const [query, file] of [
  ["", "windows-x64-setup.exe"],
  ["?arch=arm64", "windows-arm64-setup.exe"],
  ["?format=zip", "windows-x64.zip"],
  ["?arch=arm64&format=zip", "windows-arm64.zip"],
]) {
  const versioned = `moon-${release.tagName}-${file}`;
  const expected = release.assets.find((asset) => asset.name === versioned);
  if (!expected?.digest) {
    throw new Error(`Missing release digest for ${versioned}`);
  }

  const url = new URL(`/download${query}`, website);
  if (!links.has(url.href)) throw new Error(`Homepage is missing ${url}`);
  const response = await fetch(url, {
    redirect: "manual",
    signal: AbortSignal.timeout(20_000),
  });
  const download = response.headers.get("Location");
  if (response.status !== 307 || download !== `${latest}/moon-${file}`) {
    throw new Error(
      `${url} returned HTTP ${response.status} with Location ${download}`,
    );
  }

  const binary = await fetch(download, { signal: AbortSignal.timeout(90_000) });
  if (!binary.ok || !binary.body) {
    throw new Error(`${download} returned HTTP ${binary.status}`);
  }
  const hash = new Bun.CryptoHasher("sha256");
  for await (const chunk of binary.body) hash.update(chunk);
  if (`sha256:${hash.digest("hex")}` !== expected.digest) {
    throw new Error(`${url} served bytes that differ from ${versioned}`);
  }
  console.log(`${url} verified against ${versioned}`);
}
