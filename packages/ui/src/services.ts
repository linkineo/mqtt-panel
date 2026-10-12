import type { EngineClient, Platform } from '@mqttpanel/engine-client';

// The engine and platform services are chosen once at startup (Tauri in the desktop shell, the mock in the
// browser and in tests) and reached through these accessors everywhere else.
let engineRef: EngineClient | null = null;
let platformRef: Platform | null = null;

export function setServices(engine: EngineClient, platform: Platform): void {
  engineRef = engine;
  platformRef = platform;
}

export function engine(): EngineClient {
  if (!engineRef) throw new Error('engine not initialised');
  return engineRef;
}

export function platform(): Platform {
  if (!platformRef) throw new Error('platform not initialised');
  return platformRef;
}
