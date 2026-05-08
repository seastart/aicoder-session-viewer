import type { ContentBlock, Message } from "../types";

export interface SessionSearchMatch {
  messageIndex: number;
  blockIndex: number;
}

export interface SearchShortcutEvent {
  key: string;
  metaKey?: boolean;
  ctrlKey?: boolean;
  altKey?: boolean;
  shiftKey?: boolean;
}

/**
 * 在单个 session 的可读内容中查找命中块。
 *
 * 第一性原理上，定位功能只应该索引用户真正能在聊天区看到的文本；
 * 图片和 tool_use 的结构化入参暂不纳入，避免跳到用户看不到关键词的位置。
 */
export function findSessionSearchMatches(
  messages: Message[],
  rawQuery: string,
): SessionSearchMatch[] {
  const query = rawQuery.trim().toLowerCase();
  if (!query) {
    return [];
  }

  const matches: SessionSearchMatch[] = [];

  messages.forEach((message, messageIndex) => {
    message.content.forEach((block, blockIndex) => {
      const text = getSearchableBlockText(block);
      if (text.toLowerCase().includes(query)) {
        matches.push({ messageIndex, blockIndex });
      }
    });
  });

  return matches;
}

export function getSearchableBlockText(block: ContentBlock): string {
  switch (block.type) {
    case "text":
    case "thinking":
      return block.text;
    case "code":
      return block.code;
    case "tool_result":
      return block.content;
    case "image":
    case "tool_use":
      return "";
  }
}

/** 判断是否是聚焦当前会话搜索框的快捷键。 */
export function isSessionSearchShortcut(event: SearchShortcutEvent): boolean {
  return (
    event.key.toLowerCase() === "f" &&
    !event.altKey &&
    !event.shiftKey &&
    Boolean(event.metaKey || event.ctrlKey)
  );
}
