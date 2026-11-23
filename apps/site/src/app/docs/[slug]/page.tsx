import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { MDXRemote } from "next-mdx-remote/rsc";
import { getAllDocsMeta, getDocBySlug } from "@/lib/docs";

export async function generateStaticParams() {
  return getAllDocsMeta().map((doc) => ({ slug: doc.slug }));
}

export async function generateMetadata({
  params,
}: {
  params: Promise<{ slug: string }>;
}): Promise<Metadata> {
  const { slug } = await params;
  const doc = getDocBySlug(slug);
  if (!doc) {
    return { title: "Not Found" };
  }
  return {
    title: doc.title,
    description: doc.description,
  };
}

export default async function DocArticlePage({
  params,
}: {
  params: Promise<{ slug: string }>;
}) {
  const { slug } = await params;
  const doc = getDocBySlug(slug);

  if (!doc) {
    notFound();
  }

  return (
    <article className="max-w-3xl">
      <div className="border-b border-white/[0.08] pb-6">
        <p className="text-xs font-semibold uppercase tracking-widest text-violet-400">
          {doc.section}
        </p>
        <h1 className="mt-2 text-3xl font-semibold tracking-tight text-white">
          {doc.title}
        </h1>
        {doc.description && (
          <p className="mt-3 text-sm leading-relaxed text-zinc-400 sm:text-base">
            {doc.description}
          </p>
        )}
      </div>

      <div className="mdx-prose mt-8">
        <MDXRemote source={doc.content} />
      </div>
    </article>
  );
}
