import { useMemo } from "react";

export const normalizePath = (p: string) =>
  p.replace(/\\/g, "/").replace(/\/+$/, "");
export const inFolder = (file: string, folder: string) =>
  !folder ||
  normalizePath(file)
    .slice(0, normalizePath(file).lastIndexOf("/"))
    .toLowerCase() === normalizePath(folder).toLowerCase();
type Node = { path: string; name: string; children: Map<string, Node> };
export function FolderTree({
  paths,
  roots = [],
  selected = "",
  onSelect,
}: {
  paths: string[];
  roots?: string[];
  selected?: string;
  onSelect: (p: string) => void;
}) {
  const trees = useMemo(() => {
  const dirs = [...new Set(paths.map((p) =>
    normalizePath(p).split("/").slice(0, -1).join("/"),
  ))];
  let base = roots.map(normalizePath);
  if (!base.length && dirs.length) {
    let common = dirs[0].split("/");
    for (const d of dirs) {
      const parts = d.split("/");
      while (
        common.length &&
        !common.every((p, i) => parts[i]?.toLowerCase() === p.toLowerCase())
      )
        common.pop();
    }
    base = [common.join("/") || dirs[0]];
  }
  return base.map((root) => {
    const node: Node = {
      path: root,
      name: root.split("/").pop() || root,
      children: new Map(),
    };
    for (const dir of dirs) {
      if (!dir.toLowerCase().startsWith(root.toLowerCase() + "/")) continue;
      let at = node;
      for (const name of dir.slice(root.length + 1).split("/")) {
        const key = name.toLowerCase();
        if (!at.children.has(key))
          at.children.set(key, {
            path: at.path + "/" + name,
            name,
            children: new Map(),
          });
        at = at.children.get(key)!;
      }
    }
    return node;
  });
  }, [paths, roots]);
  const draw = (node: Node, depth: number): React.ReactNode => (
    <details key={node.path} open={depth === 0 || normalizePath(selected).toLowerCase().startsWith(node.path.toLowerCase() + "/") ? true : undefined}>
      <summary
        className={
          normalizePath(selected).toLowerCase() === node.path.toLowerCase()
            ? "selected-folder"
            : ""
        }
        onClick={() => onSelect(node.path)}
        title={node.path}
      >
        📁 {node.name}
      </summary>
      <div className="folder-children">
        {[...node.children.values()]
          .sort((a, b) => a.name.localeCompare(b.name))
          .map((n) => draw(n, depth + 1))}
      </div>
    </details>
  );
  return (
    <nav className="folder-tree" aria-label="Folder navigation">
      <button onClick={() => onSelect("")}>All tracks</button>
      {trees.map((n) => draw(n, 0))}
    </nav>
  );
}
