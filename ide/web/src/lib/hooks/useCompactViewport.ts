import { useMountEffect } from "@/lib/hooks/useMountEffect";
import { useState } from "react";

const COMPACT_QUERY = "(max-width: 1100px)";

export function useCompactViewport(): boolean {
  const [compact, setCompact] = useState(() =>
    typeof window !== "undefined" ? window.matchMedia(COMPACT_QUERY).matches : false
  );

  useMountEffect(() => {
    const media = window.matchMedia(COMPACT_QUERY);
    const onChange = (event: MediaQueryListEvent) => setCompact(event.matches);
    setCompact(media.matches);
    media.addEventListener("change", onChange);
    return () => media.removeEventListener("change", onChange);
  });

  return compact;
}
