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

function DownloadLink({ architecture }: { architecture: "x64" | "Arm64" }) {
  const href = architecture === "Arm64" ? "/download?arch=arm64" : "/download";
  return (
    <a className="download-button" href={href}>
      <ArrowDownToLine size={18} />
      Download for {architecture}
    </a>
  );
}

const screens = [
  {
    name: "Overview",
    file: "/screens/overview.webp",
    icon: LayoutGrid,
    description: "Pick a window from the list to jump to its workspace.",
    alt: "Moon Overview showing Code and Arc on workspace 1 and another Code window on workspace 2.",
  },
  {
    name: "Shortcuts",
    file: "/screens/shortcuts.webp",
    icon: Keyboard,
    description:
      "Click the crescent at the top of your screen to look up a shortcut.",
    alt: "Moon Shortcuts filtered to focus commands, showing their Alt key combinations.",
  },
  {
    name: "Appearance",
    file: "/screens/appearance.webp",
    icon: Palette,
    description:
      "Choose a theme or try a different border colour. Click Save changes when the preview looks right.",
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
                Free and open source
              </p>
              <h1 id="hero-title">
                Window tiling <br />
                <em>for Windows 11</em>
              </h1>
              <p className="hero-lead">
                Moon puts your editor and browser side by side as you open them.
                Hold Alt and tap the left or right arrow to switch between the
                two.
              </p>
              <div className="hero-actions">
                <DownloadLink architecture="x64" />
                <DownloadLink architecture="Arm64" />
                <a className="hero-guide" href="#quick-start">
                  Quick start <ArrowDown size={16} />
                </a>
              </div>
              <div className="platform-info">
                <Monitor size={14} /> Windows 11
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
              <h2 id="quick-title">Install Moon</h2>
              <ol className="setup-steps">
                <li>
                  <p>
                    Download and open the <a href="/download">x64 installer</a>{" "}
                    or <a href="/download?arch=arm64">Arm64 installer</a> for
                    your Windows 11 PC.
                  </p>
                </li>
                <li>
                  <p>
                    Choose <strong>Install</strong>. Moon installs for your account
                    without PowerShell or administrator approval.
                  </p>
                </li>
                <li>
                  <p>
                    Exit any other window manager and choose <strong>Open Moon</strong>.
                    Open two windows on the same workspace to try tiling.
                  </p>
                </li>
              </ol>
              <p>
                To manage apps running as administrator or update the Program
                Files edition, use the{" "}
                <a href={`${docs}/installation.md`}>UIAccess installation guide</a>.
                This edition is also available as an{" "}
                <a href="/download?format=zip">x64 ZIP</a> or{" "}
                <a href="/download?arch=arm64&format=zip">Arm64 ZIP</a>.
              </p>
            </div>
            <div className="install-stage">
              <div className="stage-pattern" aria-hidden="true" />
              <div className="install-screen">
                <ScreenPreview
                  name="Moon Settings"
                  file="/screens/appearance.webp"
                  width={1680}
                  height={1140}
                  alt="Moon Settings showing the window border and theme controls."
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
              <h2 id="navigation-title">Getting around</h2>
              <p className="section-lead">
                The orange border shows which window gets your keystrokes. Click
                a shortcut in the demo to see what changes.
              </p>
              <div className="shortcut-notes">
                <div>
                  <span>
                    <ScanSearch size={16} /> Find a window
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
                  Press <strong>Ctrl + Alt + Shift + F12</strong> to close Moon
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
                <h2 id="settings-title">Moon Settings</h2>
                <p className="settings-intro">
                  Search for Moon in Start to reopen Settings.
                </p>
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
