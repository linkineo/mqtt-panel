import { createMockEngineClient, createMockPlatform, type MockEngineClient, type MockPlatform } from '@mqttpanel/engine-client/mock';
import { startApp } from '../boot';
import { useApp } from '../store/app';

export interface Booted {
  engine: MockEngineClient;
  platform: MockPlatform;
}

/** Starts the app state against a fresh mock engine (no live traffic, no latency), in English. */
export async function boot(scenario: 'demo' | 'empty' = 'demo', lang: 'en' | 'fr' | 'de' = 'en'): Promise<Booted> {
  const engine = createMockEngineClient({ scenario, lang, live: false, latencyMs: 0 });
  const platform = createMockPlatform();
  await startApp(engine, platform, { lang });
  return { engine, platform };
}

export async function connect(id: string): Promise<void> {
  const st = useApp.getState();
  const out = await (await import('../services')).engine().connectProfile(id);
  if (out.kind !== 'connected') throw new Error('connect failed: ' + out.kind);
  const { engine } = await import('../services');
  useApp.setState({ activeConnId: id, connections: await engine().listConnections(), profiles: await engine().listProfiles() });
  void st;
}
