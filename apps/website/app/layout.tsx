import type { Metadata, Viewport } from "next";
import { GeistSans } from "geist/font/sans";
import { GeistMono } from "geist/font/mono";
import "./globals.css";

const title = "Moon — Tiling window manager for Windows 11";
const description =
  "Moon arranges your windows as you open apps on Windows 11. Download the x64 ZIP or try the keyboard shortcuts in your browser.";

export const metadata: Metadata = {
  metadataBase: new URL("https://moon.hongy.dev"),
  title,
  description,
  alternates: { canonical: "/" },
  openGraph: {
    title,
    description,
    url: "/",
    type: "website",
    siteName: "Moon",
    images: ["/share.jpg"],
  },
  twitter: {
    card: "summary_large_image",
    title,
    description,
    images: ["/share.jpg"],
  },
  other: { "site-kind": "software", "page-type": "software" },
};

export const viewport: Viewport = { themeColor: "#121311" };

export default function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html
      lang="en"
      data-type="software"
      className={`${GeistSans.variable} ${GeistMono.variable}`}
    >
      <body>{children}</body>
    </html>
  );
}
