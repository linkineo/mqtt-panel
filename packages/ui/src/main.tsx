import { createRoot } from 'react-dom/client';
import './theme.css';
import 'uplot/dist/uPlot.min.css';
import { App } from './App';
import { createServices, defaultLang, readOverrides, startApp } from './boot';

async function main() {
  const o = readOverrides(window.location.search);
  const { engine, platform } = await createServices(o.mock, defaultLang(o));
  await startApp(engine, platform, o);
  createRoot(document.getElementById('root')!).render(<App />);
}

void main();
