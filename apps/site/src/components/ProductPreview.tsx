"use client";

import { useState } from "react";
import { motion } from "motion/react";
import {
  Film,
  Scissors,
  Radio,
  UploadCloud,
  Settings,
  HardDrive,
  ShieldCheck,
} from "lucide-react";

type PreviewTab = "library" | "editor" | "recording";

const TABS: Array<{ id: PreviewTab; label: string; icon: typeof Film }> = [
  { id: "library", label: "Library", icon: Film },
  { id: "editor", label: "Editor", icon: Scissors },
  { id: "recording", label: "Recording", icon: Radio },
];

export function ProductPreview() {
  const [activeTab, setActiveTab] = useState<PreviewTab>("editor");

  return (
    <div className="relative mx-auto w-full max-w-5xl overflow-hidden rounded-2xl border border-white/[0.1] bg-[#0d0d10] shadow-[0_24px_80px_-20px_rgba(0,0,0,0.9)]">
      {/* Subtle violet top glow */}
      <div
        aria-hidden="true"
        className="pointer-events-none absolute inset-x-0 top-0 h-px bg-gradient-to-r from-transparent via-violet-500/60 to-transparent"
      />

      {/* Topbar (48px) */}
      <div className="flex h-12 items-center justify-between border-b border-white/[0.07] bg-[#111115] px-4">
        <div className="flex items-center gap-2">
          <span className="h-2.5 w-2.5 rounded-full bg-zinc-700" />
          <span className="h-2.5 w-2.5 rounded-full bg-zinc-700" />
          <span className="h-2.5 w-2.5 rounded-full bg-zinc-700" />
          <span className="ml-3 font-mono text-xs text-zinc-400">
            Obsidian Forge — ClipForge Desktop
          </span>
        </div>
        <div className="flex items-center gap-2">
          <span className="inline-flex items-center gap-1.5 rounded-full border border-emerald-500/25 bg-emerald-500/10 px-2.5 py-0.5 text-[11px] font-medium text-emerald-300">
            <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" />
            Replay Armed (F9)
          </span>
        </div>
      </div>

      {/* Main Window Body */}
      <div className="grid min-h-[340px] grid-cols-[56px_1fr] sm:grid-cols-[64px_1fr]">
        {/* Left Navigation Rail */}
        <div className="flex flex-col items-center justify-between border-r border-white/[0.07] bg-[#0b0b0e] py-3">
          <div className="flex flex-col gap-2">
            {TABS.map((tab) => {
              const Icon = tab.icon;
              const isActive = activeTab === tab.id;
              return (
                <button
                  key={tab.id}
                  type="button"
                  onClick={() => setActiveTab(tab.id)}
                  aria-label={`Preview ${tab.label} view`}
                  className={`relative flex h-10 w-10 items-center justify-center rounded-xl transition-colors ${
                    isActive
                      ? "bg-violet-500/15 text-violet-300"
                      : "text-zinc-500 hover:bg-white/[0.04] hover:text-zinc-300"
                  }`}
                >
                  <Icon className="h-4 w-4" aria-hidden="true" />
                  {isActive && (
                    <motion.span
                      layoutId="preview-rail-indicator"
                      className="absolute -left-1.5 h-5 w-1 rounded-r-full bg-violet-500"
                      transition={{ type: "spring", stiffness: 380, damping: 30 }}
                    />
                  )}
                </button>
              );
            })}
            <div className="flex h-10 w-10 items-center justify-center rounded-xl text-zinc-600">
              <UploadCloud className="h-4 w-4" aria-hidden="true" />
            </div>
          </div>
          <div className="flex h-10 w-10 items-center justify-center rounded-xl text-zinc-600">
            <Settings className="h-4 w-4" aria-hidden="true" />
          </div>
        </div>

        {/* Center Workspace */}
        <div className="flex flex-col justify-between p-4 sm:p-6">
          {activeTab === "editor" && (
            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <div>
                  <p className="text-xs font-medium uppercase tracking-wider text-violet-400">
                    Editor &bull; AutoMix Multi-Track
                  </p>
                  <h3 className="mt-0.5 font-mono text-sm text-zinc-200">
                    replay_2026-11-22_23-41-10.mp4
                  </h3>
                </div>
                <div className="flex items-center gap-2">
                  <span className="rounded-md border border-white/[0.08] bg-white/[0.03] px-2.5 py-1 font-mono text-[11px] text-zinc-300">
                    Fast Trim (-c copy)
                  </span>
                  <span className="rounded-md bg-violet-600 px-2.5 py-1 text-[11px] font-medium text-white">
                    Export Clip
                  </span>
                </div>
              </div>

              {/* Video Canvas */}
              <div className="relative flex h-40 items-center justify-center overflow-hidden rounded-xl border border-white/[0.07] bg-gradient-to-br from-[#14141b] via-[#0e0e13] to-[#171226]">
                <div className="bg-hairline-grid absolute inset-0 opacity-60" />
                <div className="relative flex flex-col items-center gap-2 text-center">
                  <div className="flex h-10 w-10 items-center justify-center rounded-full border border-violet-500/30 bg-violet-500/15 text-violet-300">
                    <Scissors className="h-4 w-4" aria-hidden="true" />
                  </div>
                  <p className="font-mono text-xs text-zinc-400">
                    Off-UI-thread FFmpeg RGBA8 Pipe + cpal Audio
                  </p>
                </div>
              </div>

              {/* Timeline */}
              <div className="space-y-2 rounded-xl border border-white/[0.07] bg-[#111116] p-3">
                <div className="flex items-center justify-between font-mono text-[11px] text-zinc-500">
                  <span>IN 00:12.40</span>
                  <span>T1: System &bull; T2: Mic (AutoMix)</span>
                  <span>OUT 00:42.00</span>
                </div>
                <div className="relative h-7 overflow-hidden rounded-lg bg-zinc-900/90">
                  <div className="absolute inset-y-0 left-[18%] right-[22%] rounded border border-violet-400/60 bg-violet-500/20" />
                  <motion.div
                    className="absolute inset-y-0 w-0.5 bg-violet-400"
                    animate={{ left: ["20%", "76%", "20%"] }}
                    transition={{
                      duration: 8,
                      repeat: Infinity,
                      ease: "linear",
                    }}
                  />
                </div>
              </div>
            </div>
          )}

          {activeTab === "library" && (
            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <div>
                  <p className="text-xs font-medium uppercase tracking-wider text-violet-400">
                    Local SQLite Library
                  </p>
                  <h3 className="mt-0.5 text-sm font-medium text-zinc-200">
                    Indexed on disk &bull; Protected by .clipforge_trash
                  </h3>
                </div>
                <span className="rounded-md border border-white/[0.08] bg-white/[0.03] px-2.5 py-1 font-mono text-[11px] text-zinc-400">
                  rusqlite v3
                </span>
              </div>
              <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
                {[
                  {
                    name: "replay_23-41.mp4",
                    tag: "Captured",
                    meta: "1080p60 • H.264",
                  },
                  {
                    name: "session_match.mp4",
                    tag: "Captured",
                    meta: "1440p60 • T1+T2",
                  },
                  {
                    name: "archive_demo.mp4",
                    tag: "Imported",
                    meta: "Disk Protected",
                  },
                ].map((clip) => (
                  <div
                    key={clip.name}
                    className="rounded-xl border border-white/[0.07] bg-[#131318] p-3"
                  >
                    <div className="mb-2.5 h-20 rounded-lg border border-white/[0.05] bg-gradient-to-br from-zinc-900 to-violet-950/20" />
                    <p className="truncate font-mono text-xs text-zinc-200">
                      {clip.name}
                    </p>
                    <div className="mt-1 flex items-center justify-between text-[11px] text-zinc-500">
                      <span>{clip.meta}</span>
                      <span className="text-violet-400">{clip.tag}</span>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}

          {activeTab === "recording" && (
            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <div>
                  <p className="text-xs font-medium uppercase tracking-wider text-violet-400">
                    Native Capture Engine
                  </p>
                  <h3 className="mt-0.5 text-sm font-medium text-zinc-200">
                    Linux GSR 6.x / Windows WGC + WASAPI
                  </h3>
                </div>
                <span className="rounded-md border border-violet-500/30 bg-violet-500/15 px-2.5 py-1 font-mono text-[11px] text-violet-300">
                  Hardware Encoded
                </span>
              </div>
              <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                <div className="rounded-xl border border-white/[0.07] bg-[#131318] p-4">
                  <p className="text-xs font-medium text-zinc-400">
                    Audio Routing
                  </p>
                  <p className="mt-1 font-mono text-sm text-zinc-100">
                    Auto / T1–T4 Discrete Tracks
                  </p>
                  <p className="mt-2 text-xs text-zinc-500">
                    Deterministic PipeWire/PulseAudio &amp; WASAPI loopback
                    separation.
                  </p>
                </div>
                <div className="rounded-xl border border-white/[0.07] bg-[#131318] p-4">
                  <p className="text-xs font-medium text-zinc-400">
                    Global Hotkeys
                  </p>
                  <p className="mt-1 font-mono text-sm text-zinc-100">
                    XDG Portal / X11 / RegisterHotKey
                  </p>
                  <p className="mt-2 text-xs text-zinc-500">
                    Save rolling replay buffers without leaving your game.
                  </p>
                </div>
              </div>
            </div>
          )}

          {/* Status Bar (28px) */}
          <div className="mt-4 flex flex-wrap items-center justify-between gap-2 border-t border-white/[0.07] pt-3 text-[11px] text-zinc-500">
            <span className="inline-flex items-center gap-1.5">
              <HardDrive className="h-3 w-3 text-zinc-400" aria-hidden="true" />
              Local-First Storage
            </span>
            <span className="inline-flex items-center gap-1.5">
              <ShieldCheck
                className="h-3 w-3 text-violet-400"
                aria-hidden="true"
              />
              Zero Telemetry &bull; MIT Open Source
            </span>
          </div>
        </div>
      </div>
    </div>
  );
}
