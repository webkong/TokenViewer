#ifndef TokenViewer_Bridging_Header_h
#define TokenViewer_Bridging_Header_h

#include <stdint.h>

typedef struct CoreHandle CoreHandle;

CoreHandle* _Nullable tt_init(const char* _Nonnull db_path);
char* _Nullable tt_sync_all(CoreHandle* _Nullable handle);
char* _Nullable tt_rebuild_all(CoreHandle* _Nullable handle);
char* _Nullable tt_get_provider_status(CoreHandle* _Nullable handle);
char* _Nullable tt_query_summary(CoreHandle* _Nullable handle, const char* _Nonnull from, const char* _Nonnull to);
char* _Nullable tt_query_daily(CoreHandle* _Nullable handle, const char* _Nonnull from, const char* _Nonnull to);
char* _Nullable tt_query_hourly(CoreHandle* _Nullable handle, const char* _Nonnull from, const char* _Nonnull to);
char* _Nullable tt_query_model_breakdown(CoreHandle* _Nullable handle, const char* _Nonnull from, const char* _Nonnull to);
char* _Nullable tt_query_heatmap(CoreHandle* _Nullable handle, int32_t weeks);
void tt_free_string(char* _Nullable ptr);
void tt_destroy(CoreHandle* _Nullable handle);

char* _Nullable tt_skills_list(CoreHandle* _Nullable handle);
char* _Nullable tt_skills_list_for_agents(CoreHandle* _Nullable handle, const char* _Nonnull json);
char* _Nullable tt_skills_list_agents(CoreHandle* _Nullable handle);
char* _Nullable tt_skills_organize(CoreHandle* _Nullable handle, const char* _Nonnull json);
char* _Nullable tt_skills_delete(CoreHandle* _Nullable handle, const char* _Nonnull json);
char* _Nullable tt_skills_restore(CoreHandle* _Nullable handle, const char* _Nonnull json);
char* _Nullable tt_skills_link(CoreHandle* _Nullable handle, const char* _Nonnull json);
char* _Nullable tt_skills_unlink(CoreHandle* _Nullable handle, const char* _Nonnull json);
char* _Nullable tt_skills_git_status(CoreHandle* _Nullable handle);
char* _Nullable tt_skills_git_pull(CoreHandle* _Nullable handle);
char* _Nullable tt_skills_git_push(CoreHandle* _Nullable handle);
char* _Nullable tt_skills_git_connectivity(CoreHandle* _Nullable handle);
char* _Nullable tt_skills_add_custom_agent(CoreHandle* _Nullable handle, const char* _Nonnull json);
char* _Nullable tt_skills_remove_custom_agent(CoreHandle* _Nullable handle, const char* _Nonnull json);
char* _Nullable tt_skills_get_config(CoreHandle* _Nullable handle);
char* _Nullable tt_skills_set_git_config(CoreHandle* _Nullable handle, const char* _Nonnull json);

#endif
