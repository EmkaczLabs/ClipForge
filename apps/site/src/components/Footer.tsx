import Link from "next/link";

const FOOTER_COLUMNS = [
  {
    title: "Product",
    links: [
      { href: "/features", label: "Features" },
      { href: "/download", label: "Download" },
      { href: "/changelog", label: "Changelog" },
      { href: "/roadmap", label: "Roadmap" },
    ],
  },
  {
    title: "Resources",
    links: [
      { href: "/docs", label: "Documentation" },
      { href: "/server", label: "ClipForge-Server" },
      { href: "/status", label: "System Status" },
    ],
  },
  {
    title: "Project",
    links: [
      { href: "/open-source", label: "Open Source (MIT)" },
      { href: "/privacy", label: "Privacy" },
      {
        href: "https://github.com/ClipForge/ClipForge",
        label: "ClipForge GitHub",
        external: true,
      },
      {
        href: "https://github.com/EmkaczLabs/ClipForge-Server",
        label: "Server GitHub",
        external: true,
      },
    ],
  },
];

export function Footer() {
  return (
    <footer className="border-t border-white/[0.08] bg-[#09090b]">
      <div className="mx-auto max-w-6xl px-4 py-12 sm:px-6">
        <div className="grid grid-cols-1 gap-8 sm:grid-cols-2 md:grid-cols-4">
          <div className="space-y-3">
            <div className="flex items-center gap-2 text-sm font-semibold text-zinc-100">
              <span className="h-2 w-2 rounded-full bg-violet-500" />
              <span>ClipForge</span>
            </div>
            <p className="text-xs leading-relaxed text-zinc-500">
              Local-first desktop game &amp; screen capture built in Rust.
              Capture the moment. Keep the memory.
            </p>
          </div>

          {FOOTER_COLUMNS.map((col) => (
            <div key={col.title} className="space-y-3">
              <h3 className="text-xs font-semibold uppercase tracking-wider text-zinc-400">
                {col.title}
              </h3>
              <ul className="space-y-2 text-sm">
                {col.links.map((link) =>
                  "external" in link && link.external ? (
                    <li key={link.href}>
                      <a
                        href={link.href}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="text-zinc-500 transition-colors hover:text-zinc-200"
                      >
                        {link.label}
                      </a>
                    </li>
                  ) : (
                    <li key={link.href}>
                      <Link
                        href={link.href}
                        className="text-zinc-500 transition-colors hover:text-zinc-200"
                      >
                        {link.label}
                      </Link>
                    </li>
                  )
                )}
              </ul>
            </div>
          ))}
        </div>

        <div className="mt-10 flex flex-col items-start justify-between gap-4 border-t border-white/[0.06] pt-6 text-xs text-zinc-500 sm:flex-row sm:items-center">
          <p>MIT Licensed &bull; Built with Rust, Rune, FFmpeg &amp; SQLite.</p>
          <p>Zero telemetry. Local-first by design.</p>
        </div>
      </div>
    </footer>
  );
}
