import assert from "node:assert/strict";
import {
  findSessionSearchMatches,
  isSessionSearchShortcut,
} from "./sessionSearch.ts";
import type { Message } from "../types.ts";

const baseMessage: Omit<Message, "id" | "content"> = {
  role: "assistant",
  timestamp: null,
  model: null,
  usage: null,
};

function message(id: string, content: Message["content"]): Message {
  return {
    ...baseMessage,
    id,
    content,
  };
}

const messages: Message[] = [
  message("first", [{ type: "text", text: "Install dependencies with pnpm." }]),
  message("second", [
    { type: "tool_use", tool_name: "exec", tool_id: "1", input: { cmd: "pnpm build" } },
    { type: "tool_result", tool_id: "1", content: "Build failed in ChatView.tsx", is_error: true },
  ]),
  message("third", [{ type: "code", language: "tsx", code: "export function ChatView() {}" }]),
];

assert.deepEqual(
  findSessionSearchMatches(messages, "chatview"),
  [
    { messageIndex: 1, blockIndex: 1 },
    { messageIndex: 2, blockIndex: 0 },
  ],
);

assert.deepEqual(findSessionSearchMatches(messages, "   "), []);

assert.equal(isSessionSearchShortcut({ key: "f", metaKey: true }), true);
assert.equal(isSessionSearchShortcut({ key: "F", ctrlKey: true }), true);
assert.equal(isSessionSearchShortcut({ key: "f" }), false);
assert.equal(
  isSessionSearchShortcut({ key: "f", metaKey: true, altKey: true }),
  false,
);
