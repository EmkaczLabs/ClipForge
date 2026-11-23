import type { Metadata } from "next";
import { Inter, JetBrains_Mono } from "next/font/google";
import "./globals.css";
import { Navbar } from "@/components/Navbar";
import { Footer } from "@/components/Footer";
import { MotionProvider } from "@/components/MotionPrimitives";

const inter = Inter({
  subsets: ["latin"],
  variable: "--font-sans",
  display: "swap",
});

const jetbrainsMono = JetBrains_Mono({
  subsets: ["latin"],
  variable: "--font-mono",
  display: "swap",
});

export const metadata: Metadata = {
  title: {
    default: "ClipForge — Capture the moment. Keep the memory.",
    template: "%s — ClipForge",
  },
  description:
    "Local-first, open-source desktop clip & replay recorder built with Rust, Rune, FFmpeg, and gpu-screen-recorder.",
  metadataBase: new URL(
    process.env.NEXT_PUBLIC_SITE_URL || "https://clipforge.emkacz.dev"
  ),
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html
      lang="en"
      className={`${inter.variable} ${jetbrainsMono.variable} dark`}
    >
      <body className="flex min-h-screen flex-col bg-[#09090b] text-zinc-100 selection:bg-violet-500/30 selection:text-violet-100">
        <MotionProvider>
          <Navbar />
          <main className="flex-1">{children}</main>
          <Footer />
        </MotionProvider>
      </body>
    </html>
  );
}
