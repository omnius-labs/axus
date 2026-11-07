import { useState } from "react";
import { describeError } from "./api";
import { messages } from "./messages";
import { percent } from "./status";

export const buttonClass = "bg-black px-3 py-1 text-white disabled:opacity-50";
export const dangerClass = "border border-red-600 px-3 py-1 text-red-600 disabled:opacity-50";

export function ErrorText({ error }: { error: unknown }) {
  if (!error) return null;
  return (
    <p role="alert" className="text-red-600">
      {describeError(error)}
    </p>
  );
}

export function ProgressBar({ done, total }: { done: number; total: number }) {
  const value = percent(done, total);
  return (
    <div className="flex items-center gap-2">
      <progress className="w-40" value={value} max={100} aria-label={messages.common.progress} />
      <span className="tabular-nums">{value}%</span>
    </div>
  );
}

export function formatTime(value: string) {
  return new Date(value).toLocaleString();
}

export function CopyBox({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="flex items-start gap-2">
      <pre className="flex-1 overflow-x-auto border bg-gray-50 p-2 font-mono text-xs">{text}</pre>
      <button
        type="button"
        className={buttonClass}
        onClick={async () => {
          await navigator.clipboard.writeText(text);
          setCopied(true);
        }}
      >
        {copied ? messages.common.copied : messages.common.copy}
      </button>
    </div>
  );
}

export function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="grid grid-cols-[10rem_1fr] gap-2 border-b py-1">
      <dt className="text-gray-600">{label}</dt>
      <dd className="break-all">{children}</dd>
    </div>
  );
}
