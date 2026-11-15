// Generated files are checked in so native builds do not require Bun on Windows.
const { mkdir, copyFile } = await import("node:fs/promises");
await mkdir("dist", { recursive: true });
await Promise.all(
  ["index.html", "styles.css"].map((name) =>
    copyFile(`src/${name}`, `dist/${name}`),
  ),
);

await copyFile("../../crates/moon-win/assets/icons/moon.svg", "dist/moon.svg");

await copyFile(
  "../../crates/moon-win/assets/SymbolsNerdFont-Regular.ttf",
  "dist/moon-symbols.ttf",
);
