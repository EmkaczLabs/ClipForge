import { getAllDocsMeta } from "@/lib/docs";
import { DocsSidebar } from "@/components/DocsSidebar";

export default function DocsLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const docs = getAllDocsMeta();

  return (
    <div className="mx-auto max-w-6xl px-4 py-12 sm:px-6 lg:py-16">
      <div className="flex flex-col gap-10 lg:flex-row lg:gap-12">
        <DocsSidebar docs={docs} />
        <div className="min-w-0 flex-1">{children}</div>
      </div>
    </div>
  );
}
