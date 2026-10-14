// Tauri adapter: each EngineClient method maps to the Rust command of the same name in snake_case.
// Generated from client.ts by the lead; keep in sync with apps/desktop/src-tauri/src/commands.rs.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Menu, MenuItem, PredefinedMenuItem, Submenu } from '@tauri-apps/api/menu';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import type { ContextMenuItem, EngineClient, Platform } from './client';
import type { EngineEvent } from './types';

/* eslint-disable @typescript-eslint/no-explicit-any */
export function createTauriEngineClient(): EngineClient {
  const client: Omit<EngineClient, 'onEvent'> = {
    appInfo: () => invoke('app_info', {}),
    getSettings: () => invoke('get_settings', {}),
    saveSettings: (settings) => invoke('save_settings', { settings }),
    listProfiles: () => invoke('list_profiles', {}),
    newProfile: () => invoke('new_profile', {}),
    saveProfile: (profile) => invoke('save_profile', { profile }),
    deleteProfile: (profileId) => invoke('delete_profile', { profileId }),
    duplicateProfile: (profileId) => invoke('duplicate_profile', { profileId }),
    reorderProfiles: (ids) => invoke('reorder_profiles', { ids }),
    setPassword: (profileId, password) => invoke('set_password', { profileId, password }),
    profilePassword: (profileId) => invoke('profile_password', { profileId }),
    importProfiles: (path) => invoke('import_profiles', { path }),
    exportProfiles: (passphrase) => invoke('export_profiles', { passphrase }),
    pickFile: (kind) => invoke('pick_file', { kind }),
    inspectFile: (kind, path) => invoke('inspect_file', { kind, path }),
    setProfileFile: (profileId, field, path, passphrase) => invoke('set_profile_file', { profileId, field, path, passphrase }),
    addProtoSchema: (profileId, path, topicFilter, messageType) => invoke('add_proto_schema', { profileId, path, topicFilter, messageType }),
    removeProtoSchema: (profileId, fileName) => invoke('remove_proto_schema', { profileId, fileName }),
    connectProfile: (profileId, skipOnce) => invoke('connect_profile', { profileId, skipOnce: skipOnce ?? false }),
    testProfile: (profileId, skipOnce) => invoke('test_profile', { profileId, skipOnce: skipOnce ?? false }),
    trustCertificate: (profileId, mode) => invoke('trust_certificate', { profileId, mode }),
    disconnect: (connectionId) => invoke('disconnect', { connectionId }),
    closeConnection: (connectionId) => invoke('close_connection', { connectionId }),
    listConnections: () => invoke('list_connections', {}),
    treeSlice: (connectionId, query, start, count) => invoke('tree_slice', { connectionId, query, start, count }),
    tableSlice: (connectionId, query, start, count) => invoke('table_slice', { connectionId, query, start, count }),
    setExpanded: (connectionId, path, expanded) => invoke('set_expanded', { connectionId, path, expanded }),
    expandToDepth: (connectionId, depth) => invoke('expand_to_depth', { connectionId, depth }),
    expandChanged: (connectionId, withinMs) => invoke('expand_changed', { connectionId, withinMs }),
    collapseAll: (connectionId) => invoke('collapse_all', { connectionId }),
    getExpanded: (connectionId) => invoke('get_expanded', { connectionId }),
    setExpandedSet: (connectionId, paths) => invoke('set_expanded_set', { connectionId, paths }),
    setPinned: (connectionId, paths) => invoke('set_pinned', { connectionId, paths }),
    revealTopic: (connectionId, topic) => invoke('reveal_topic', { connectionId, topic }),
    nextChanged: (connectionId, fromTopic, withinMs) => invoke('next_changed', { connectionId, fromTopic, withinMs }),
    knownTopics: (connectionId, prefix, limit) => invoke('known_topics', { connectionId, prefix, limit }),
    topicDetail: (connectionId, topic) => invoke('topic_detail', { connectionId, topic }),
    message: (connectionId, topic, seq, format, formatAnyway) => invoke('message', { connectionId, topic, seq, format, formatAnyway }),
    history: (connectionId, topic) => invoke('history', { connectionId, topic }),
    diff: (connectionId, topic, baselineSeq, seq) => invoke('diff', { connectionId, topic, baselineSeq, seq }),
    setTopicBuffer: (connectionId, topic, size) => invoke('set_topic_buffer', { connectionId, topic, size }),
    clearHistory: (connectionId, prefix) => invoke('clear_history', { connectionId, prefix }),
    retainedUnder: (connectionId, prefix) => invoke('retained_under', { connectionId, prefix }),
    memory: () => invoke('memory', {}),
    series: (connectionId, topic, path, sinceMs) => invoke('series', { connectionId, topic, path, sinceMs }),
    exportEstimate: (connectionId, options) => invoke('export_estimate', { connectionId, options }),
    exportData: (connectionId, options) => invoke('export_data', { connectionId, options }),
    exportChartCsv: (connectionId, series, sinceMs) => invoke('export_chart_csv', { connectionId, series, sinceMs }),
    savePng: (defaultName, base64) => invoke('save_png', { defaultName, base64 }),
    exportSentLog: (connectionId) => invoke('export_sent_log', { connectionId }),
    publish: (connectionId, request) => invoke('publish', { connectionId, request }),
    deleteRetained: (connectionId, topic) => invoke('delete_retained', { connectionId, topic }),
    deleteRetainedMany: (connectionId, topics) => invoke('delete_retained_many', { connectionId, topics }),
    sentLog: (connectionId) => invoke('sent_log', { connectionId }),
    clearSentLog: (connectionId) => invoke('clear_sent_log', { connectionId }),
    startRepeat: (connectionId, request, intervalS, count) => invoke('start_repeat', { connectionId, request, intervalS, count }),
    stopRepeat: (connectionId) => invoke('stop_repeat', { connectionId }),
    listTemplates: (profileId) => invoke('list_templates', { profileId }),
    saveTemplate: (profileId, template) => invoke('save_template', { profileId, template }),
    deleteTemplate: (profileId, templateId) => invoke('delete_template', { profileId, templateId }),
    encodePayload: (format, payload) => invoke('encode_payload', { format, payload }),
    brokerStatus: () => invoke('broker_status', {}),
    brokerConfig: () => invoke('broker_config', {}),
    brokerStart: () => invoke('broker_start', {}),
    brokerStop: () => invoke('broker_stop', {}),
    brokerRestart: () => invoke('broker_restart', {}),
    validatePort: (port, listenerId) => invoke('validate_port', { port, listenerId }),
    addListener: (request) => invoke('add_listener', { request }),
    updateListener: (listener) => invoke('update_listener', { listener }),
    removeListener: (listenerId) => invoke('remove_listener', { listenerId }),
    setBrokerBind: (bind) => invoke('set_broker_bind', { bind }),
    setAnonymous: (on) => invoke('set_anonymous', { on }),
    setRequireClientCert: (on) => invoke('set_require_client_cert', { on }),
    setVerifyAgainst: (source) => invoke('set_verify_against', { source }),
    addUser: (username, password) => invoke('add_user', { username, password }),
    setUserPassword: (username, password) => invoke('set_user_password', { username, password }),
    removeUser: (username) => invoke('remove_user', { username }),
    brokerUserPassword: (username) => invoke('broker_user_password', { username }),
    issuedCerts: () => invoke('issued_certs', {}),
    issueClientCert: (request) => invoke('issue_client_cert', { request }),
    importCsr: (name, validYears) => invoke('import_csr', { name, validYears }),
    exportClientCert: (name, format, passphrase) => invoke('export_client_cert', { name, format, passphrase }),
    revokeClientCert: (name) => invoke('revoke_client_cert', { name }),
    brokerCerts: () => invoke('broker_certs', {}),
    exportCa: () => invoke('export_ca', {}),
    regenerateBrokerCerts: (sans, keyType) => invoke('regenerate_broker_certs', { sans, keyType }),
    importBrokerCert: () => invoke('import_broker_cert', {}),
    importClientCa: () => invoke('import_client_ca', {}),
    rotateCa: () => invoke('rotate_ca', {}),
    brokerClients: () => invoke('broker_clients', {}),
    disconnectClient: (clientId) => invoke('disconnect_client', { clientId }),
    brokerLog: (level, afterSeq) => invoke('broker_log', { level, afterSeq }),
    appBrokerStep: (listenerId, step, profileName) => invoke('app_broker_step', { listenerId, step, profileName }),
  };
  return {
    ...client,
    onEvent(handler: (event: EngineEvent) => void) {
      let unlisten: (() => void) | null = null;
      let cancelled = false;
      listen<EngineEvent>('engine-event', (e) => handler(e.payload)).then((u) => {
        if (cancelled) u();
        else unlisten = u;
      });
      return () => {
        cancelled = true;
        if (unlisten) unlisten();
      };
    },
  };
}

export function createTauriPlatform(): Platform {
  return {
    async showContextMenu(items: ContextMenuItem[], x: number, y: number) {
      return new Promise<string | null>(async (resolve) => {
        let chosen: string | null = null;
        // Native submenus for items with children (for example "New connection" > "This app's broker").
        const build = (list: ContextMenuItem[]): Promise<(MenuItem | PredefinedMenuItem | Submenu)[]> =>
          Promise.all(
            list.map(async (it) =>
              it.separator
                ? PredefinedMenuItem.new({ item: 'Separator' })
                : it.items
                  ? Submenu.new({ id: it.id, text: it.label, enabled: it.enabled !== false, items: await build(it.items) })
                  : MenuItem.new({
                      id: it.id,
                      text: it.label,
                      enabled: it.enabled !== false,
                      action: () => {
                        chosen = it.id;
                      },
                    }),
            ),
          );
        const entries = await build(items);
        const menu = await Menu.new({ items: entries });
        const { LogicalPosition } = await import('@tauri-apps/api/dpi');
        await menu.popup(new LogicalPosition(x, y));
        // popup() resolves when the menu closes; the action callback (if any) has run by then or runs right after.
        setTimeout(() => resolve(chosen), 30);
      });
    },
    onFileDrop(handler) {
      let unlisten: (() => void) | null = null;
      let cancelled = false;
      getCurrentWebview()
        .onDragDropEvent((event) => {
          const p = event.payload as any;
          if (p.type === 'drop') {
            const ratio = window.devicePixelRatio || 1;
            handler(p.paths as string[], p.position.x / ratio, p.position.y / ratio);
          }
        })
        .then((u) => {
          if (cancelled) u();
          else unlisten = u;
        });
      return () => {
        cancelled = true;
        if (unlisten) unlisten();
      };
    },
    async copyText(text: string) {
      await navigator.clipboard.writeText(text);
    },
    prefersReducedMotion() {
      return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    },
    async setMenuLanguage(lang: string) {
      await invoke('set_menu_language', { lang });
    },
  };
}
