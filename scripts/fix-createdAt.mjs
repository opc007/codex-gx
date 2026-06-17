// 批量给 Composer.tsx 中缺 createdAt 的多行对象添加 createdAt
import { readFileSync, writeFileSync } from "node:fs";

const path = "apps/desktop/src/components/Composer.tsx";
let text = readFileSync(path, "utf8");

// 多行模式：
//   id: crypto.randomUUID(),
//   role: "assistant",
//   text:
// → 在 role 后插入 createdAt: Date.now(),
const re = /(\n            id: crypto\.randomUUID\(\),)(\n            role: "assistant",)(\n            text:)/g;
const before = text.length;
text = text.replace(re, "$1$2\n            createdAt: Date.now(),$3");
const after = text.length;

writeFileSync(path, text);
console.log(`before=${before} after=${after}`);
