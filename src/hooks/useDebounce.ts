import { useEffect, useRef } from "react";

/** 防抖 hook：延迟执行回调 */
export function useDebounce(
  callback: () => void,
  delay: number,
  deps: readonly unknown[]
) {
  const callbackRef = useRef(callback);
  callbackRef.current = callback;

  useEffect(() => {
    const timer = setTimeout(() => callbackRef.current(), delay);
    return () => clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [delay, ...deps]);
}
