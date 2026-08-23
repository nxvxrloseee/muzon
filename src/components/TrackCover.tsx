import { Music2 } from "lucide-react";
import { useEffect, useState } from "react";

const SCHEME = "muzon-cover";

/** Custom protocols are served from `scheme://localhost` everywhere except
 * Windows, where the webview only allows an http origin. */
const ORIGIN = navigator.userAgent.includes("Windows")
  ? `http://${SCHEME}.localhost`
  : `${SCHEME}://localhost`;

/** base64url rather than percent-encoding: a track path can contain anything,
 * and this survives the webview's URL parsing untouched on every platform. */
function encodePath(path: string): string {
  const bytes = new TextEncoder().encode(path);
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

export function coverUrl(path: string): string {
  return `${ORIGIN}/${encodePath(path)}`;
}

/**
 * Cover art for list rows. Fetching it as a base64 data URL through `invoke`
 * (see `useTrackCover`) costs one IPC round trip per visible row and hands
 * WebKit an image it cannot cache; a real URL lets it cache the bytes, decode
 * off the main thread, and drop them again under memory pressure - which is
 * what makes scrolling a large library cheap.
 *
 * `useTrackCover` survives only for the tag editor, which shows the current
 * artwork next to a file picker and wants the bytes it is about to replace.
 */
export function TrackCover({
  path,
  className,
  iconSize = 16,
}: {
  path: string | null | undefined;
  /** Classes for the container box - it should center its content, since the
   * placeholder icon is laid out inside it. */
  className?: string;
  iconSize?: number;
}) {
  // A track with no art at all answers 404; fall back to the placeholder
  // rather than leaving a broken image behind.
  const [failed, setFailed] = useState(false);
  useEffect(() => setFailed(false), [path]);

  return (
    <div className={className}>
      {path && !failed ? (
        <img
          src={coverUrl(path)}
          alt=""
          loading="lazy"
          decoding="async"
          className="h-full w-full object-cover"
          onError={() => setFailed(true)}
        />
      ) : (
        <Music2 size={iconSize} className="text-text-secondary/40" />
      )}
    </div>
  );
}
