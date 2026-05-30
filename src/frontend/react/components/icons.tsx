/**
 * Stroke-only icons sized to the Cntrl Panel scale (default 17×17 for
 * nav/products; pass `size` for everything else). All icons inherit
 * `currentColor` and use a 1.6 stroke-width to match the Tabler default.
 * Copyright (c) 2026-present Tabler icons and Nadhi.dev, licensed under MIT.
 * Source: https://tabler-icons.io/
 */
type IconProps = {
  size?: number;
  className?: string;
  strokeWidth?: number;
};

const base = (props: IconProps) => ({
  width: props.size ?? 17,
  height: props.size ?? 17,
  viewBox: "0 0 24 24",
  fill: "none" as const,
  stroke: "currentColor",
  strokeWidth: props.strokeWidth ?? 1.6,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  className: props.className,
  "aria-hidden": true,
});

export function IconChat(props: IconProps) {
  return (
    <svg {...base(props)}>
      <path d="M21 12c0 4.418-4.03 8-9 8a9.7 9.7 0 0 1-2.4-.3L4 21l1.4-3.6A7.5 7.5 0 0 1 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8Z" />
    </svg>
  );
}

export function IconHash(props: IconProps) {
  return (
    <svg {...base(props)}>
      <path d="M5 9h14M5 15h14M10 3l-2 18M16 3l-2 18" />
    </svg>
  );
}

export function IconSend(props: IconProps) {
  return (
    <svg {...base(props)}>
      <path d="M3 11.5 21 3l-7 18-2.5-7.5L3 11.5Z" />
    </svg>
  );
}

export function IconSignOut(props: IconProps) {
  return (
    <svg {...base(props)}>
      <path d="M14 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2v-2" />
      <path d="M20 12H10M17 9l3 3-3 3" />
    </svg>
  );
}

export function IconUser(props: IconProps) {
  return (
    <svg {...base(props)}>
      <circle cx="12" cy="8" r="4" />
      <path d="M4 21a8 8 0 0 1 16 0" />
    </svg>
  );
}

export function IconLock(props: IconProps) {
  return (
    <svg {...base(props)}>
      <rect x="4" y="11" width="16" height="10" rx="2" />
      <path d="M8 11V7a4 4 0 0 1 8 0v4" />
    </svg>
  );
}

export function IconSparkle(props: IconProps) {
  return (
    <svg {...base(props)}>
      <path d="M12 3v4M12 17v4M3 12h4M17 12h4M5.6 5.6l2.8 2.8M15.6 15.6l2.8 2.8M5.6 18.4l2.8-2.8M15.6 8.4l2.8-2.8" />
    </svg>
  );
}

export function IconArrowRight(props: IconProps) {
  return (
    <svg {...base(props)}>
      <path d="M5 12h14M13 6l6 6-6 6" />
    </svg>
  );
}

export function IconDot(props: IconProps) {
  return (
    <svg {...base(props)}>
      <circle cx="12" cy="12" r="3" />
    </svg>
  );
}

export function IconPlus(props: IconProps) {
  return (
    <svg {...base(props)}>
      <path d="M12 5v14M5 12h14" />
    </svg>
  );
}

export function IconTrash(props: IconProps) {
  return (
    <svg {...base(props)}>
      <path d="M4 7h16M10 11v6M14 11v6M6 7l1 13a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-13M9 7V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v3" />
    </svg>
  );
}

export function IconMenu(props: IconProps) {
  return (
    <svg {...base(props)}>
      <path d="M4 6h16M4 12h16M4 18h16" />
    </svg>
  );
}

export function IconClose(props: IconProps) {
  return (
    <svg {...base(props)}>
      <path d="M6 6l12 12M18 6l-12 12" />
    </svg>
  );
}

export function IconBolt(props: IconProps) {
  return (
    <svg {...base(props)}>
      <path d="M13 2 4 14h7l-1 8 9-12h-7l1-8Z" />
    </svg>
  );
}

export function IconSidebarCollapse(props: IconProps) {
  return (
    <svg {...base(props)}>
      <rect x="3" y="4" width="18" height="16" rx="2.5" />
      <path d="M9 4v16" />
      <path d="M15 10l-2 2 2 2" />
    </svg>
  );
}

export function IconSidebarExpand(props: IconProps) {
  return (
    <svg {...base(props)}>
      <rect x="3" y="4" width="18" height="16" rx="2.5" />
      <path d="M9 4v16" />
      <path d="M13 10l2 2-2 2" />
    </svg>
  );
}

export function IconPencil(props: IconProps) {
  return (
    <svg {...base(props)}>
      <path d="M4 20h4l11-11-4-4L4 16v4Z" />
      <path d="M14 6l4 4" />
    </svg>
  );
}

export function IconChevronDown(props: IconProps) {
  return (
    <svg {...base(props)}>
      <path d="m6 9 6 6 6-6" />
    </svg>
  );
}

export function IconCode(props: IconProps) {
  return (
    <svg {...base(props)}>
      <path d="m9 8-5 4 5 4M15 8l5 4-5 4M13 5l-3 14" />
    </svg>
  );
}

export function IconBook(props: IconProps) {
  return (
    <svg {...base(props)}>
      <path d="M4 5a2 2 0 0 1 2-2h13v16H6a2 2 0 0 0-2 2V5Z" />
      <path d="M4 19a2 2 0 0 0 2 2h13" />
    </svg>
  );
}

export function IconLightbulb(props: IconProps) {
  return (
    <svg {...base(props)}>
      <path d="M9 18h6M10 21h4" />
      <path d="M12 3a6 6 0 0 0-4 10.5c.6.5 1 1.2 1 2V15h6v-.5c0-.8.4-1.5 1-2A6 6 0 0 0 12 3Z" />
    </svg>
  );
}

export function IconCompass(props: IconProps) {
  return (
    <svg {...base(props)}>
      <circle cx="12" cy="12" r="9" />
      <path d="m15 9-2 5-5 2 2-5 5-2Z" />
    </svg>
  );
}

export function IconSearch(props: IconProps) {
  return (
    <svg {...base(props)}>
      <circle cx="11" cy="11" r="7" />
      <path d="m20 20-3.5-3.5" />
    </svg>
  );
}

export function IconAlert(props: IconProps) {
  return (
    <svg {...base(props)}>
      <path d="M10.3 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0Z" />
      <path d="M12 9v4M12 17h.01" />
    </svg>
  );
}

export function IconFolder(props: IconProps) {
  return (
    <svg {...base(props)}>
      <path d="M5 4h4l2 3h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Z" />
    </svg>
  );
}

export function IconDotsVertical(props: IconProps) {
  return (
    <svg {...base(props)}>
      <circle cx="12" cy="5" r="1" fill="currentColor" stroke="none" />
      <circle cx="12" cy="12" r="1" fill="currentColor" stroke="none" />
      <circle cx="12" cy="19" r="1" fill="currentColor" stroke="none" />
    </svg>
  );
}

export function IconMic(props: IconProps) {
  return (
    <svg {...base(props)}>
      <rect x="9" y="3" width="6" height="11" rx="3" />
      <path d="M5 11a7 7 0 0 0 14 0" />
      <path d="M12 18v3M8 21h8" />
    </svg>
  );
}

export function IconMicOff(props: IconProps) {
  return (
    <svg {...base(props)}>
      <line x1="3" y1="3" x2="21" y2="21" />
      <path d="M9 9v2a3 3 0 0 0 5.12 2.12M15 9.34V5a3 3 0 0 0-5.94-.6" />
      <path d="M19 11a7 7 0 0 1-1.1 3.74M5 11a7 7 0 0 0 10.3 6.18" />
      <path d="M12 18v3M8 21h8" />
    </svg>
  );
}

export function IconWaveform(props: IconProps) {
  return (
    <svg {...base(props)}>
      <path d="M4 10v4M8 6v12M12 4v16M16 7v10M20 10v4" />
    </svg>
  );
}

export function IconStop(props: IconProps) {
  return (
    <svg {...base(props)}>
      <rect x="6" y="6" width="12" height="12" rx="2" />
    </svg>
  );
}

export function IconVolume(props: IconProps) {
  return (
    <svg {...base(props)}>
      <path d="M11 5 6 9H2v6h4l5 4V5z" />
      <path d="M15.5 8.5a5 5 0 0 1 0 7M18.5 5.5a9 9 0 0 1 0 13" />
    </svg>
  );
}

export function IconVolumeOff(props: IconProps) {
  return (
    <svg {...base(props)}>
      <path d="M11 5 6 9H2v6h4l5 4V5z" />
      <path d="M22 9l-6 6M16 9l6 6" />
    </svg>
  );
}
