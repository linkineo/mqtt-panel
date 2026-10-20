/** The chart canvas as a PNG in base64, painted on the theme background (the uPlot canvas is transparent). Used by
 * "Export…, PNG" on the Charts screen and by the assistant's export_chart. */
export function chartPngBase64(canvas: HTMLCanvasElement): string {
  const c2 = document.createElement('canvas');
  c2.width = canvas.width;
  c2.height = canvas.height;
  const g = c2.getContext('2d');
  if (g) {
    g.fillStyle = getComputedStyle(document.documentElement).getPropertyValue('--surface').trim() || '#fff';
    g.fillRect(0, 0, c2.width, c2.height);
    g.drawImage(canvas, 0, 0);
  }
  return c2.toDataURL('image/png').split(',')[1] ?? '';
}

/** The canvas of the chart on the Charts screen, once it is drawn (polls up to `ms`). */
export async function waitForChartCanvas(ms = 3000): Promise<HTMLCanvasElement | null> {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    const c = document.querySelector<HTMLCanvasElement>('[data-testid="chart-pane"] canvas');
    if (c && c.width > 0) {
      // a moment more, so the latest points are drawn (a timer, not requestAnimationFrame: frames pause while the
      // window is behind another app, which is when an assistant asks)
      await new Promise((r) => setTimeout(r, 150));
      return c;
    }
    await new Promise((r) => setTimeout(r, 50));
  }
  return null;
}
