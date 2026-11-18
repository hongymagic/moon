import {
  ArrowDown,
  ArrowDownToLine,
  ArrowUpRight,
  Keyboard,
  LayoutGrid,
  Monitor,
  Palette,
  ScanSearch,
  Undo2,
} from "lucide-react";
import LunarPlate from "../components/lunar-plate";
import ScreenPreview from "../components/screen-preview";
import WorkspaceDemo from "../components/workspace-demo";

const repository = "https://github.com/hongymagic/moon";
const docs = `${repository}/blob/main/docs`;

function Wordmark() {
  return (
    <span className="wordmark">
      <img src="/icon.svg" alt="" width="36" height="36" />
      Moon
    </span>
  );
}

function DownloadLink() {
  return (
    <a className="download-button" href="/download">
      <ArrowDownToLine size={18} />
      Download for Windows
    </a>
  );
}

const screens = [
  {
    name: "Overview",
    file: "/screens/overview.webp",
    icon: LayoutGrid,
    description:
      "Find an open window across workspaces and bring it into focus.",
    alt: "Moon Overview showing Code and Arc on workspace 1 and another Code window on workspace 2.",
  },
  {
    name: "Shortcuts",
    file: "/screens/shortcuts.webp",
    icon: Keyboard,
    description:
      "Click the crescent in the bar to see your current key bindings.",
    alt: "Moon Shortcuts filtered to focus commands, showing their Alt key combinations.",
  },
  {
    name: "Appearance",
    file: "/screens/appearance.webp",
    icon: Palette,
    description:
      "Set your gaps, border colors, and theme. Preview them as you go.",
    alt: "Moon Appearance with a live border preview and Light, Dark, and System theme options.",
  },
];

export default function Home() {
  return (
    <>
      <a className="skip-link" href="#main">
        Skip to content
      </a>
      <header className="masthead" id="top">
        <div className="wrap nav-inner">
          <a className="home-link" href="#top" aria-label="Moon home">
            <Wordmark />
          </a>
          <nav aria-label="Primary navigation">
            <a href="#quick-start">Quick start</a>
            <a href="#navigation">Navigation</a>
            <a href="#settings">Settings</a>
          </nav>
          <a className="repo-link" href={repository}>
            GitHub <ArrowUpRight size={15} />
          </a>
        </div>
      </header>
      <main id="main">
        <section className="hero" aria-labelledby="hero-title" data-enter>
          <div className="wrap hero-inner">
            <div className="hero-copy">
              <p className="eyebrow hero-eyebrow">
                <span className="status-light" />
                Tiling window manager
              </p>
              <h1 id="hero-title">
                Your desktop,
                <br />
                <em>in orbit.</em>
              </h1>
              <p className="hero-lead">
                Moon tiles your windows as you open them. Move between apps and
                workspaces on Windows 11 with a few keystrokes.
              </p>
              <div className="hero-actions">
                <DownloadLink />
                <a className="hero-guide" href="#quick-start">
                  Quick start <ArrowDown size={16} />
                </a>
              </div>
              <div className="platform-info">
                <Monitor size={14} /> Windows 11 x64 <span>·</span> Open source{" "}
                <span>·</span> MIT
              </div>
            </div>
            <LunarPlate />
          </div>
        </section>

        <section
          id="quick-start"
          className="quick-section"
          aria-labelledby="quick-title"
        >
          <div className="wrap quick-grid">
            <div className="setup-copy">
              <p className="eyebrow">
                <ArrowDownToLine size={14} /> Quick start
              </p>
              <h2 id="quick-title">Set up Moon.</h2>
              <ol className="setup-steps">
                <li>
                  <div>
                    <h3>Download and extract</h3>
                    <p>
                      Get the <a href="/download">Windows x64 ZIP</a>. In File
                      Explorer, right-click it and choose{" "}
                      <strong>Extract All</strong>.
                    </p>
                  </div>
                </li>
                <li>
                  <div>
                    <h3>Run the installer</h3>
                    <p>
                      Double-click <strong>Install Moon.cmd</strong> in the
                      extracted folder. Choose <strong>Install Moon</strong> and
                      approve the Windows prompt.
                    </p>
                  </div>
                </li>
                <li>
                  <div>
                    <h3>Open two windows</h3>
                    <p>
                      Choose <strong>Open Settings</strong>. If prompted,
                      install WebView2 and return to setup. Open two app windows
                      on the same workspace to see Moon tile them.
                    </p>
                  </div>
                </li>
              </ol>
              <a className="text-link" href={`${docs}/installation.md`}>
                Installation and recovery guide <ArrowUpRight size={16} />
              </a>
            </div>
            <div className="install-stage">
              <div className="stage-pattern" aria-hidden="true" />
              <div className="install-screen">
                <ScreenPreview
                  name="Moon installer"
                  file="/screens/install.webp"
                  width={775}
                  height={600}
                  alt="Moon setup for Windows, with Install Moon selected and a description of what happens next."
                />
              </div>
            </div>
          </div>
        </section>

        <section
          id="navigation"
          className="navigation-section"
          aria-labelledby="navigation-title"
        >
          <div className="wrap nav-grid">
            <div className="navigation-copy">
              <p className="eyebrow">
                <Keyboard size={15} /> Navigation
              </p>
              <h2 id="navigation-title">Try the controls.</h2>
              <p className="section-lead">
                Focus a neighbor, move a tile, or switch workspaces. Try each
                shortcut in the demo.
              </p>
              <div className="shortcut-notes">
                <div>
                  <span>
                    <ScanSearch size={16} /> Find any window
                  </span>
                  <span>
                    <kbd>Ctrl</kbd> <kbd>Alt</kbd> <kbd>Space</kbd>
                  </span>
                </div>
                <div>
                  <span>
                    <Undo2 size={16} /> Undo a layout change
                  </span>
                  <span>
                    <kbd>Ctrl</kbd> <kbd>Alt</kbd> <kbd>Z</kbd>
                  </span>
                </div>
              </div>
              <details className="exit-note">
                <summary>How do I stop Moon?</summary>
                <p>
                  Press <strong>Ctrl + Alt + Shift + F12</strong> to stop Moon
                  and restore your windows.
                </p>
              </details>
            </div>
            <WorkspaceDemo />
          </div>
        </section>

        <section
          id="settings"
          className="settings-section"
          aria-labelledby="settings-title"
        >
          <div className="wrap">
            <div className="settings-heading">
              <div>
                <p className="eyebrow">
                  <LayoutGrid size={14} /> Settings
                </p>
                <h2 id="settings-title">Inside Moon.</h2>
              </div>
              <a className="text-link" href={`${docs}/config-reference.md`}>
                Configuration reference <ArrowUpRight size={16} />
              </a>
            </div>
            <div className="screen-gallery">
              {screens.map(({ icon: Icon, ...screen }) => (
                <article className="screen-card" key={screen.name}>
                  <div className={`gallery-stage ${screen.name.toLowerCase()}`}>
                    <ScreenPreview
                      name={screen.name}
                      file={screen.file}
                      alt={screen.alt}
                      width={1680}
                      height={1140}
                    />
                  </div>
                  <h3>
                    <Icon size={19} />
                    {screen.name}
                  </h3>
                  <p>{screen.description}</p>
                </article>
              ))}
            </div>
          </div>
        </section>
      </main>
      <footer>
        <div className="wrap footer-inner">
          <a className="home-link" href="#top" aria-label="Moon, back to top">
            <Wordmark />
          </a>
          <div className="footer-links">
            <a href={`${repository}/releases/latest`}>
              Releases <ArrowUpRight size={13} />
            </a>
            <a href={`${repository}/blob/main/LICENSE`}>
              MIT license <ArrowUpRight size={13} />
            </a>
            <a href={repository}>
              GitHub <ArrowUpRight size={13} />
            </a>
          </div>
          <a
            className="image-credit"
            href="https://science.nasa.gov/photojournal/nearside-spectacular/"
          >
            Lunar image · NASA/GSFC/ASU
          </a>
        </div>
      </footer>
    </>
  );
}
