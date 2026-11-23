"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useMemo, useState } from "react";
import { Search } from "lucide-react";
import type { DocMeta } from "@/lib/docs";

export function DocsSidebar({ docs }: { docs: DocMeta[] }) {
  const pathname = usePathname();
  const [query, setQuery] = useState("");

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return docs;
    return docs.filter(
      (doc) =>
        doc.title.toLowerCase().includes(q) ||
        doc.description.toLowerCase().includes(q) ||
        doc.searchableText.toLowerCase().includes(q)
    );
  }, [docs, query]);

  const sections = useMemo(() => {
    const grouped = new Map<string, DocMeta[]>();
    for (const doc of filtered) {
      const list = grouped.get(doc.section) ?? [];
      list.push(doc);
      grouped.set(doc.section, list);
    }
    return Array.from(grouped.entries());
  }, [filtered]);

  return (
    <aside className="w-full shrink-0 lg:w-64">
      <div className="sticky top-24 space-y-6">
        <div className="relative">
          <Search
            className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-zinc-500"
            aria-hidden="true"
          />
          <input
            type="search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search docs..."
            aria-label="Search documentation"
            className="w-full rounded-xl border border-white/[0.1] bg-[#111114] py-2 pl-9 pr-3 text-sm text-zinc-100 placeholder:text-zinc-500 focus:border-violet-500/60 focus:outline-none"
          />
        </div>

        <nav aria-label="Documentation sections" className="space-y-6">
          {sections.length === 0 ? (
            <p className="text-xs text-zinc-500">No matching docs found.</p>
          ) : (
            sections.map(([section, items]) => (
              <div key={section} className="space-y-2">
                <h2 className="px-2 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">
                  {section}
                </h2>
                <ul className="space-y-1">
                  {items.map((doc) => {
                    const href = `/docs/${doc.slug}`;
                    const active = pathname === href;
                    return (
                      <li key={doc.slug}>
                        <Link
                          href={href}
                          className={`block rounded-lg px-2.5 py-1.5 text-sm transition-colors ${
                            active
                              ? "bg-violet-500/15 font-medium text-violet-300"
                              : "text-zinc-400 hover:bg-white/[0.04] hover:text-zinc-200"
                          }`}
                        >
                          {doc.title}
                        </Link>
                      </li>
                    );
                  })}
                </ul>
              </div>
            ))
          )}
        </nav>
      </div>
    </aside>
  );
}
