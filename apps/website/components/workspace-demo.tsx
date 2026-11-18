"use client";

import { Tabs, TabsList, TabsPanel, TabsTab } from "@ui-sdk/react/tabs";
import { useState } from "react";
import type { ReactNode } from "react";

import "./workspace-demo.css";

const lessons = [
  {
    id: "focus",
    title: "Focus",
    detail:
      "Follow the border from one window to the other. Both windows stay where they are.",
  },
  {
    id: "move",
    title: "Move",
    detail: "The two windows trade places. You keep typing in the same one.",
  },
  {
    id: "switch",
    title: "Switch",
    detail:
      "Hold Alt and press a workspace number. On your desktop, you can also click its number in Moon’s top bar.",
  },
] as const;

type Lesson = (typeof lessons)[number]["id"];
type WindowId = "editor" | "notes" | "photo" | "files";
type WorkspaceNumber = 1 | 2;
type Workspace = { windows: [WindowId, WindowId]; focused: WindowId };
type Desktop = {
  active: WorkspaceNumber;
  spaces: Record<WorkspaceNumber, Workspace>;
};

const initialDesktop: Desktop = {
  active: 1,
  spaces: {
    1: { windows: ["editor", "notes"], focused: "editor" },
    2: { windows: ["photo", "files"], focused: "photo" },
  },
};

const windowTitles: Record<WindowId, string> = {
  editor: "config.toml",
  notes: "Field notes.txt",
  photo: "Lunar surface.webp",
  files: "Pictures",
};

function runShortcut(desktop: Desktop, lesson: Lesson): Desktop {
  if (lesson === "switch") {
    return { ...desktop, active: desktop.active === 1 ? 2 : 1 };
  }
  const { windows, focused } = desktop.spaces[desktop.active];
  const [left, right] = windows;
  const updated: Workspace =
    lesson === "move"
      ? { windows: [right, left], focused }
      : { windows, focused: focused === left ? right : left };
  return {
    ...desktop,
    spaces: { ...desktop.spaces, [desktop.active]: updated },
  };
}

const configLines: ReactNode[] = [
  <>
    <i># Desktop layout</i>
  </>,
  <>
    config-version = <b>1</b>
  </>,
  <>
    theme = <em>"dark"</em>
  </>,
  null,
  <strong>[gaps]</strong>,
  <>
    inner = <b>16</b>
  </>,
  <>
    outer = <b>32</b>
  </>,
  null,
  <strong>[borders]</strong>,
  <>
    width = <b>4</b>
  </>,
];

function TileContent({ id }: { id: WindowId }) {
  switch (id) {
    case "editor":
      return (
        <div className="md-code">
          {configLines.map((line, index) => (
            <div key={index}>
              <small>{index + 1}</small>
              <span>{line}</span>
            </div>
          ))}
        </div>
      );
    case "notes":
      return (
        <div className="md-paper">
          <div className="md-menu">
            File <span>Edit</span> View
          </div>
          <div className="md-writing">
            <strong>Field notes</strong>
            <p>Look for the shadow along the crater rim.</p>
            <span className="md-caret" />
          </div>
        </div>
      );
    case "photo":
      return (
        <div className="md-photo">
          <span>100%</span>
        </div>
      );
    case "files":
      return (
        <div className="md-files">
          <div className="md-location">
            Home <span>›</span> Pictures
          </div>
          <div className="md-file">
            <span className="md-thumbnail" />
            Lunar surface.webp
          </div>
          <div className="md-file">
            <span className="md-file-sheet" />
            Field notes.txt
          </div>
        </div>
      );
  }
}

function DesktopPreview({ desktop }: { desktop: Desktop }) {
  const { windows, focused } = desktop.spaces[desktop.active];
  const [left, right] = windows;
  return (
    <div
      className="md-desktop"
      role="img"
      aria-label={`Workspace ${desktop.active}: ${windowTitles[left]} on the left, ${windowTitles[right]} on the right. ${windowTitles[focused]} has focus.`}
    >
      <div className="md-bar" aria-hidden="true">
        <span className="md-crescent">☾</span>
        <div className="md-spaces">
          {Array.from({ length: 10 }, (_, i) => (
            <span key={i} data-active={i + 1 === desktop.active}>
              {i + 1}
            </span>
          ))}
        </div>
        <div className="md-bar-center">
          <span>09:41</span>
          <span>{windowTitles[focused]}</span>
        </div>
        <div className="md-providers">
          <span>↔</span>
          <span>ENG</span>
          <span>▮▮▮</span>
        </div>
      </div>
      <div className="md-tiles" aria-hidden="true" key={desktop.active}>
        {windows.map((id, index) => (
          <div
            key={id}
            className={`md-tile md-${id}-tile`}
            data-side={index === 0 ? "left" : "right"}
            data-focused={id === focused}
          >
            <div className="md-chrome">
              <span className="md-app-symbol">
                {id === "editor"
                  ? "{ }"
                  : id === "notes"
                    ? "≡"
                    : id === "photo"
                      ? "◩"
                      : "▱"}
              </span>
              <span className="md-title">{windowTitles[id]}</span>
              <span className="md-window-controls">
                <i>−</i>
                <i>□</i>
                <i>×</i>
              </span>
            </div>
            <TileContent id={id} />
          </div>
        ))}
      </div>
    </div>
  );
}

export default function WorkspaceDemo() {
  const [desktop, setDesktop] = useState(initialDesktop);
  const { windows, focused } = desktop.spaces[desktop.active];
  const onLeft = windows[0] === focused;
  const direction = onLeft ? "right" : "left";
  const destination = desktop.active === 1 ? "2" : "1";
  const actionKeys = {
    focus: ["Alt", onLeft ? "→" : "←"],
    move: ["Alt", "Shift", onLeft ? "→" : "←"],
    switch: ["Alt", destination],
  };
  const actionLabels = {
    focus: `Focus ${direction}`,
    move: `Move ${direction}`,
    switch: `Switch to ${destination}`,
  };

  return (
    <div className="moon-demo">
      <Tabs defaultValue="focus">
        <TabsList className="md-tabs" aria-label="Window manager actions">
          {lessons.map(({ id, title }) => (
            <TabsTab key={id} value={id}>
              {title}
            </TabsTab>
          ))}
        </TabsList>
        <DesktopPreview desktop={desktop} />
        {lessons.map(({ id, detail }) => (
          <TabsPanel key={id} value={id} className="md-panel">
            <p>{detail}</p>
            <button
              type="button"
              onClick={() =>
                setDesktop((previous) => runShortcut(previous, id))
              }
              aria-label={`${actionLabels[id]} (${actionKeys[id].join(" + ")})`}
            >
              <span>{actionLabels[id]}</span>
              <span className="md-keys" aria-hidden="true">
                {actionKeys[id].map((key) => (
                  <kbd key={key}>{key}</kbd>
                ))}
              </span>
            </button>
          </TabsPanel>
        ))}
      </Tabs>
      <span className="md-announcement" role="status" aria-atomic="true">
        Workspace {desktop.active}. {windowTitles[focused]} focused on the{" "}
        {onLeft ? "left" : "right"}.
      </span>
    </div>
  );
}
