import assert from "node:assert/strict";
import { afterEach, mock, test } from "node:test";
import { GET } from "../app/download/route";

const releases = "https://github.com/hongymagic/moon/releases";
const fallback = `${releases}/latest`;

function publishedRelease(tag: string) {
  const name = `moon-${tag}-windows-x64.zip`;
  return {
    tag_name: tag,
    draft: false,
    prerelease: false,
    assets: [
      {
        name,
        state: "uploaded",
        browser_download_url: `${releases}/download/${tag}/${name}`,
      },
    ],
  };
}

afterEach(() => mock.restoreAll());

test("redirects to the Windows ZIP for each new release", async () => {
  for (const tag of ["2026.11.18-0", "2027.01.02-14"]) {
    const release = publishedRelease(tag);
    const upstream = mock.method(globalThis, "fetch", async () =>
      Response.json(release),
    );
    const response = await GET();
    assert.equal(response.status, 307);
    assert.equal(
      response.headers.get("Location"),
      release.assets[0].browser_download_url,
    );
    assert.equal(response.headers.get("Cache-Control"), "no-store");
    upstream.mock.restore();
  }
});

test("uses the release page for invalid tags, missing assets, and unexpected URLs", async () => {
  const release = publishedRelease("2026.11.18-0");
  const asset = release.assets[0];
  for (const data of [
    null,
    publishedRelease("../other"),
    { ...release, prerelease: true },
    { ...release, draft: true },
    { ...release, assets: [] },
    { ...release, assets: [null, { ...asset, name: `${asset.name}.sha256` }] },
    { ...release, assets: [{ ...asset, state: "starter" }] },
    {
      ...release,
      assets: [
        { ...asset, browser_download_url: "https://example.com/file.zip" },
      ],
    },
  ]) {
    const upstream = mock.method(globalThis, "fetch", async () =>
      Response.json(data),
    );
    assert.equal((await GET()).headers.get("Location"), fallback);
    upstream.mock.restore();
  }
});

test("uses the release page when GitHub is unavailable or returns invalid JSON", async () => {
  for (const reply of [
    () => new Response(null, { status: 403 }),
    () => new Response("upstream error"),
    () => {
      throw new TypeError("network error");
    },
  ]) {
    const upstream = mock.method(globalThis, "fetch", async () => reply());
    assert.equal((await GET()).headers.get("Location"), fallback);
    upstream.mock.restore();
  }
});
