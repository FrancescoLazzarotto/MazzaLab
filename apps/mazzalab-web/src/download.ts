/** Hands a text to the browser as a file to save; a byte order mark keeps Excel from misreading UTF-8. */
export function downloadText(filename: string, text: string, type = "text/csv"): void {
  const url = URL.createObjectURL(new Blob(["﻿", text], { type: `${type};charset=utf-8` }));
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  document.body.append(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
}

/** A name that is safe as a file name on every system. */
export function safeFileName(name: string): string {
  const cleaned = name.replace(/[\/:*?"<>|\u0000-\u001f]/g, "").trim();
  return cleaned || "cues";
}
