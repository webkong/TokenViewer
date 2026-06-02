#ifndef TokenViewer_Bridging_Header_h
#define TokenViewer_Bridging_Header_h

#include <stdint.h>

typedef struct CoreHandle CoreHandle;

CoreHandle* _Nullable tt_init(const char* _Nonnull db_path);
char* _Nullable tt_sync_all(CoreHandle* _Nullable handle);
char* _Nullable tt_get_provider_status(CoreHandle* _Nullable handle);
char* _Nullable tt_query_summary(CoreHandle* _Nullable handle, const char* _Nonnull from, const char* _Nonnull to);
char* _Nullable tt_query_daily(CoreHandle* _Nullable handle, const char* _Nonnull from, const char* _Nonnull to);
char* _Nullable tt_query_hourly(CoreHandle* _Nullable handle, const char* _Nonnull from, const char* _Nonnull to);
char* _Nullable tt_query_model_breakdown(CoreHandle* _Nullable handle, const char* _Nonnull from, const char* _Nonnull to);
char* _Nullable tt_query_heatmap(CoreHandle* _Nullable handle, int32_t weeks);
void tt_free_string(char* _Nullable ptr);
void tt_destroy(CoreHandle* _Nullable handle);

#endif
