import fs from "node:fs";
import path from "node:path";
import matter from "gray-matter";

export interface DocMeta {
  slug: string;
  title: string;
  description: string;
  section: string;
  order: number;
  headings: string[];
  searchableText: string;
}

export interface DocPage extends DocMeta {
  content: string;
}

const DOCS_DIR = path.join(process.cwd(), "src", "content", "docs");

function extractHeadings(markdown: string): string[] {
  const matches = markdown.match(/^#{2,3}\s+(.+)$/gm) ?? [];
  return matches.map((line) => line.replace(/^#{2,3}\s+/, "").trim());
}

export function getAllDocsMeta(): DocMeta[] {
  if (!fs.existsSync(DOCS_DIR)) {
    return [];
  }

  const files = fs
    .readdirSync(DOCS_DIR)
    .filter((file) => file.endsWith(".mdx"));

  const docs = files.map((file) => {
    const slug = file.replace(/\.mdx$/, "");
    const raw = fs.readFileSync(path.join(DOCS_DIR, file), "utf8");
    const { data, content } = matter(raw);

    const title = typeof data.title === "string" ? data.title : slug;
    const description =
      typeof data.description === "string" ? data.description : "";
    const section =
      typeof data.section === "string" ? data.section : "General";
    const order = typeof data.order === "number" ? data.order : 99;
    const headings = extractHeadings(content);
    const searchableText = [title, description, section, ...headings, content]
      .join(" ")
      .replace(/[#*`>\[\]()]/g, " ")
      .replace(/\s+/g, " ")
      .slice(0, 4000);

    return {
      slug,
      title,
      description,
      section,
      order,
      headings,
      searchableText,
    };
  });

  return docs.sort((a, b) => a.order - b.order);
}

export function getDocBySlug(slug: string): DocPage | null {
  if (!/^[a-z0-9-]+$/.test(slug)) {
    return null;
  }
  const filePath = path.join(DOCS_DIR, `${slug}.mdx`);
  if (!fs.existsSync(filePath)) {
    return null;
  }

  const raw = fs.readFileSync(filePath, "utf8");
  const { data, content } = matter(raw);

  const title = typeof data.title === "string" ? data.title : slug;
  const description =
    typeof data.description === "string" ? data.description : "";
  const section = typeof data.section === "string" ? data.section : "General";
  const order = typeof data.order === "number" ? data.order : 99;
  const headings = extractHeadings(content);

  return {
    slug,
    title,
    description,
    section,
    order,
    headings,
    searchableText: "",
    content,
  };
}
