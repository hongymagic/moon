import assert from "node:assert/strict";
import { test } from "node:test";
import { GET } from "../app/download/route";

const latest = "https://github.com/hongymagic/moon/releases/latest";

test("each website download redirects to its permanent GitHub asset", () => {
  for (const [query, asset] of [
    ["", "moon-windows-x64-setup.exe"],
    ["?arch=x64", "moon-windows-x64-setup.exe"],
    ["?arch=arm64", "moon-windows-arm64-setup.exe"],
    ["?format=zip", "moon-windows-x64.zip"],
    ["?arch=arm64&format=zip", "moon-windows-arm64.zip"],
  ]) {
    const response = GET(new Request(`https://moon.hongy.dev/download${query}`));
    assert.equal(response.status, 307);
    assert.equal(response.headers.get("Location"), `${latest}/download/${asset}`);
    assert.equal(response.headers.get("Cache-Control"), "no-store");
  }
});

test("an unknown architecture opens the latest release page", () => {
  const response = GET(new Request("https://moon.hongy.dev/download?arch=unknown"));
  assert.equal(response.headers.get("Location"), latest);
});
