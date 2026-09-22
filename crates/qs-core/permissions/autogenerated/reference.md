## Default Permission

Default permissions for the QuantSuite core plugin: settings, the event bus, the entity/link store, the process register, window lifecycle and the agent tool catalogue. process_list is read-only — start, stop and log tail belong to the module that owns the process, under that module's own permissions.

#### This default permission set includes the following:

- `allow-get-settings`
- `allow-get-setting`
- `allow-set-setting`
- `allow-emit-event`
- `allow-recent-events`
- `allow-upsert-entity`
- `allow-delete-entity`
- `allow-list-entities`
- `allow-count-entities`
- `allow-search-entities`
- `allow-link-entities`
- `allow-unlink-entities`
- `allow-linked-entities`
- `allow-process-list`
- `allow-set-circular-window`
- `allow-window-new`
- `allow-window-show`
- `allow-window-hide`
- `allow-window-toggle`
- `allow-quit`
- `allow-autostart-enabled`
- `allow-set-autostart`
- `allow-app-version`
- `allow-suite-paths`
- `allow-agent-tools`
- `allow-agent-tool-decision`
- `allow-agent-pending-calls`
- `allow-agent-call-claim`
- `allow-agent-call-complete`

## Permission Table

<table>
<tr>
<th>Identifier</th>
<th>Description</th>
</tr>


<tr>
<td>

`qs:allow-agent-call-claim`

</td>
<td>

Enables the agent_call_claim command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-agent-call-claim`

</td>
<td>

Denies the agent_call_claim command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-agent-call-complete`

</td>
<td>

Enables the agent_call_complete command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-agent-call-complete`

</td>
<td>

Denies the agent_call_complete command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-agent-pending-calls`

</td>
<td>

Enables the agent_pending_calls command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-agent-pending-calls`

</td>
<td>

Denies the agent_pending_calls command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-agent-tool-decision`

</td>
<td>

Enables the agent_tool_decision command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-agent-tool-decision`

</td>
<td>

Denies the agent_tool_decision command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-agent-tools`

</td>
<td>

Enables the agent_tools command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-agent-tools`

</td>
<td>

Denies the agent_tools command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-app-version`

</td>
<td>

Enables the app_version command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-app-version`

</td>
<td>

Denies the app_version command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-autostart-enabled`

</td>
<td>

Enables the autostart_enabled command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-autostart-enabled`

</td>
<td>

Denies the autostart_enabled command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-count-entities`

</td>
<td>

Enables the count_entities command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-count-entities`

</td>
<td>

Denies the count_entities command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-delete-entity`

</td>
<td>

Enables the delete_entity command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-delete-entity`

</td>
<td>

Denies the delete_entity command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-emit-event`

</td>
<td>

Enables the emit_event command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-emit-event`

</td>
<td>

Denies the emit_event command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-get-setting`

</td>
<td>

Enables the get_setting command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-get-setting`

</td>
<td>

Denies the get_setting command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-get-settings`

</td>
<td>

Enables the get_settings command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-get-settings`

</td>
<td>

Denies the get_settings command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-link-entities`

</td>
<td>

Enables the link_entities command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-link-entities`

</td>
<td>

Denies the link_entities command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-linked-entities`

</td>
<td>

Enables the linked_entities command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-linked-entities`

</td>
<td>

Denies the linked_entities command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-list-entities`

</td>
<td>

Enables the list_entities command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-list-entities`

</td>
<td>

Denies the list_entities command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-open-module-window`

</td>
<td>

Enables the open_module_window command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-open-module-window`

</td>
<td>

Denies the open_module_window command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-process-list`

</td>
<td>

Enables the process_list command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-process-list`

</td>
<td>

Denies the process_list command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-quit`

</td>
<td>

Enables the quit command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-quit`

</td>
<td>

Denies the quit command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-recent-events`

</td>
<td>

Enables the recent_events command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-recent-events`

</td>
<td>

Denies the recent_events command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-search-entities`

</td>
<td>

Enables the search_entities command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-search-entities`

</td>
<td>

Denies the search_entities command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-set-autostart`

</td>
<td>

Enables the set_autostart command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-set-autostart`

</td>
<td>

Denies the set_autostart command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-set-circular-window`

</td>
<td>

Enables the set_circular_window command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-set-circular-window`

</td>
<td>

Denies the set_circular_window command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-set-setting`

</td>
<td>

Enables the set_setting command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-set-setting`

</td>
<td>

Denies the set_setting command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-suite-paths`

</td>
<td>

Enables the suite_paths command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-suite-paths`

</td>
<td>

Denies the suite_paths command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-unlink-entities`

</td>
<td>

Enables the unlink_entities command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-unlink-entities`

</td>
<td>

Denies the unlink_entities command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-upsert-entity`

</td>
<td>

Enables the upsert_entity command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-upsert-entity`

</td>
<td>

Denies the upsert_entity command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-window-hide`

</td>
<td>

Enables the window_hide command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-window-hide`

</td>
<td>

Denies the window_hide command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-window-new`

</td>
<td>

Enables the window_new command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-window-new`

</td>
<td>

Denies the window_new command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-window-show`

</td>
<td>

Enables the window_show command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-window-show`

</td>
<td>

Denies the window_show command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-window-toggle`

</td>
<td>

Enables the window_toggle command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-window-toggle`

</td>
<td>

Denies the window_toggle command without any pre-configured scope.

</td>
</tr>
</table>
