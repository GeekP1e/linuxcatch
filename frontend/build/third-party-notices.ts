import { readFileSync, readdirSync, statSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import type { Plugin } from "vite";

export function thirdPartyNotices(): Plugin {
  return {
    name: "third-party-notices",
    apply: "build",
    generateBundle(_options, bundle) {
      // Vite also adds its module-preload helper to the browser bundle.
      const require = createRequire(import.meta.url);
      const roots = new Set([dirname(require.resolve("vite/package.json"))]);
      for (const output of Object.values(bundle)) {
        if (output.type !== "chunk") continue;
        for (const id of Object.keys(output.modules)) {
          const match = id
            .replaceAll("\\", "/")
            .replace(/^\0/, "")
            .match(/^(.*\/node_modules\/(?:@[^/]+\/)?[^/]+)/);
          if (match) roots.add(match[1]);
        }
      }

      const notices = [...roots].map((root) => {
        const metadata = JSON.parse(readFileSync(join(root, "package.json"), "utf8")) as {
          name: string;
          version: string;
        };
        const files = readdirSync(root)
          .filter((name) => /^(?:licen[cs]e|notice|copying)(?:[._-].*)?$/i.test(name))
          .filter((name) => statSync(join(root, name)).isFile())
          .sort();
        if (!files.some((name) => /^(?:licen[cs]e|copying)/i.test(name))) {
          this.error(`Missing license text for bundled package ${metadata.name}`);
        }
        const text = files
          .map((name) => `${name}\n\n${readFileSync(join(root, name), "utf8").trim()}`)
          .join("\n\n");
        return { name: metadata.name, text: `${metadata.name}@${metadata.version}\n\n${text}` };
      });
      notices.sort((left, right) => left.name.localeCompare(right.name));

      this.emitFile({
        type: "asset",
        fileName: "THIRD_PARTY_NOTICES.txt",
        source: [
          "Third-party notices for the LinuxCatch frontend",
          "These terms apply to the identified third-party components only.\n" +
            "LinuxCatch's own license is provided separately in LICENSE.\n" +
            "Package license files may include notices for their bundled dependencies.",
          ...notices.map((notice) => notice.text),
        ].join("\n\n" + "=".repeat(72) + "\n\n") + "\n",
      });
    },
  };
}
