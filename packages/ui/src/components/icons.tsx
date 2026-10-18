// Rail icons, drawn as in the mockup.
const paths: Record<string, string> = {
  connect: '<path d="M6 8v4a6 6 0 0 0 12 0V8"/><path d="M9 3v5M15 3v5M12 18v3"/>',
  explore: '<path d="M4 6h16M8 12h12M12 18h8"/>',
  publish: '<path d="M4 12l16-8-6 16-2-7z"/>',
  charts: '<path d="M3 20h18M5 16l4-6 4 3 6-8"/>',
  broker: '<rect x="4" y="4" width="16" height="7" rx="1.5"/><rect x="4" y="13" width="16" height="7" rx="1.5"/><path d="M8 7.5h.01M8 16.5h.01"/>',
  settings: '<circle cx="12" cy="12" r="3"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M4.9 19.1L7 17M17 7l2.1-2.1"/>',
};

export function RailIcon({ name }: { name: string }) {
  return <svg viewBox="0 0 24 24" aria-hidden="true" dangerouslySetInnerHTML={{ __html: paths[name] ?? '' }} />;
}

const APP_MARK =
  'M0 0h1v1h-1zM2 0h1v1h-1zM4 0h1v1h-1zM5 0h1v1h-1zM6 0h1v1h-1zM8 0h1v1h-1zM1 1h1v1h-1zM3 1h1v1h-1zM5 1h1v1h-1zM6 1h1v1h-1zM7 1h1v1h-1zM9 1h1v1h-1zM0 2h1v1h-1zM2 2h1v1h-1zM4 2h1v1h-1zM6 2h1v1h-1zM7 2h1v1h-1zM8 2h1v1h-1zM1 3h1v1h-1zM3 3h1v1h-1zM5 3h1v1h-1zM7 3h1v1h-1zM8 3h1v1h-1zM9 3h1v1h-1zM0 4h1v1h-1zM2 4h1v1h-1zM4 4h1v1h-1zM6 4h1v1h-1zM8 4h1v1h-1zM9 4h1v1h-1zM0 5h1v1h-1zM1 5h1v1h-1zM3 5h1v1h-1zM5 5h1v1h-1zM7 5h1v1h-1zM9 5h1v1h-1zM0 6h1v1h-1zM1 6h1v1h-1zM2 6h1v1h-1zM4 6h1v1h-1zM6 6h1v1h-1zM8 6h1v1h-1zM1 7h1v1h-1zM2 7h1v1h-1zM3 7h1v1h-1zM5 7h1v1h-1zM7 7h1v1h-1zM9 7h1v1h-1zM0 8h1v1h-1zM2 8h1v1h-1zM3 8h1v1h-1zM4 8h1v1h-1zM6 8h1v1h-1zM8 8h1v1h-1zM1 9h1v1h-1zM3 9h1v1h-1zM4 9h1v1h-1zM5 9h1v1h-1zM7 9h1v1h-1zM9 9h1v1h-1z';

// Decorative title-bar mark: no title, no label, no text naming it (owner, 18 Oct 2026).
export function AppMark() {
  return (
    <svg viewBox="0 0 10 10" shapeRendering="crispEdges" aria-hidden="true" focusable="false">
      <path fill="currentColor" d={APP_MARK} />
    </svg>
  );
}
