import { useEffect, useState } from "react";

/**
 * 监听全局 Alt（macOS 上是 Option）键的按下状态。
 *
 * 用于在 UI 上实时反馈"按住 Alt 将以 YOLO 模式启动"。注意需要在 window blur 时
 * 把状态重置为 false——否则用户按住 Alt 切换到其它窗口再切回来，会卡在按下态。
 */
export function useAltKeyPressed(): boolean {
  const [pressed, setPressed] = useState(false);

  useEffect(() => {
    const handleDown = (e: KeyboardEvent) => {
      if (e.key === "Alt") setPressed(true);
    };
    const handleUp = (e: KeyboardEvent) => {
      if (e.key === "Alt") setPressed(false);
    };
    const handleBlur = () => setPressed(false);

    window.addEventListener("keydown", handleDown);
    window.addEventListener("keyup", handleUp);
    window.addEventListener("blur", handleBlur);

    return () => {
      window.removeEventListener("keydown", handleDown);
      window.removeEventListener("keyup", handleUp);
      window.removeEventListener("blur", handleBlur);
    };
  }, []);

  return pressed;
}
