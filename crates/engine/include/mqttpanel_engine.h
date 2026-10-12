/*
 * mqttpanel_engine.h - C ABI of the MQTT Panel engine (hand-written).
 *
 * One engine per MQTT connection. The engine does no I/O and keeps no clock: the caller passes
 * receive times and "now" in milliseconds since the Unix epoch. The engine is not thread-safe;
 * serialise calls on one handle.
 *
 * Strings are NUL-terminated UTF-8. Strings returned by the engine must be released with
 * mqttpanel_string_free.
 */
#ifndef MQTTPANEL_ENGINE_H
#define MQTTPANEL_ENGINE_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque engine handle. */
typedef struct MqttPanelEngine MqttPanelEngine;

/* Creates an engine. config_json: camelCase EngineConfig (bufferPerTopic, pointsPerSeries,
 * payloadRenderLimit, maxKeptPayload, memoryBudget, gapFactor), or NULL / "" for defaults. */
MqttPanelEngine *mqttpanel_engine_new(const char *config_json);

/* Frees an engine. The handle must not be used afterwards. */
void mqttpanel_engine_free(MqttPanelEngine *handle);

/* Ingests one PUBLISH. payload may be NULL when len is 0. Returns 0, or -1 on invalid arguments. */
int32_t mqttpanel_engine_ingest(MqttPanelEngine *handle, const char *topic, const uint8_t *payload,
                                size_t len, uint8_t qos, bool retain, int64_t received_at);

/* Runs a JSON query {"method": "...", "params": {...}} and returns {"ok": result} or
 * {"error": "..."}. Methods and params (camelCase):
 *   tree_slice   {query: TreeQuery, start, count, nowMs}
 *   table_slice  {query: TableQuery, start, count, nowMs}
 *   topic_detail {topic}
 *   series       {topic, path, sinceMs}
 *   message      {topic, seq, format, formatAnyway}
 *   history      {topic}
 *   diff         {topic, baselineSeq, seq}
 *   fields       {topic}
 *   memory       {}
 *   set_expanded {path, expanded}
 *   expand_to_depth {depth}
 *   set_pinned   {paths}
 *   seq, topic_count
 * The result must be freed with mqttpanel_string_free. */
char *mqttpanel_engine_query(MqttPanelEngine *handle, const char *request_json);

/* Frees a string returned by mqttpanel_engine_query. */
void mqttpanel_string_free(char *s);

#ifdef __cplusplus
}
#endif

#endif /* MQTTPANEL_ENGINE_H */
