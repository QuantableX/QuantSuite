## Default Permission

Default permissions for the QuantSuite core plugin: settings, the event bus, the entity/link store, the process register, window lifecycle, the agent tool catalogue and the secret vault's status (never a secret value). process_list is read-only — start, stop and log tail belong to the module that owns the process, under that module's own permissions.

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
- `allow-vault-status`
- `allow-vault-unlock`
- `allow-bundle-plan`
- `allow-bundle-export`
- `allow-bundle-export-cancel`
- `allow-import-propose`
- `allow-import-bundle`
- `allow-import-status`
- `allow-server-status`
- `allow-server-set-role`
- `allow-server-set-watchdog`
- `allow-gateway-status`
- `allow-gateway-configure`
- `allow-gateway-pairing-start`
- `allow-gateway-pairing-status`
- `allow-gateway-pairing-confirm`
- `allow-gateway-pairing-cancel`
- `allow-gateway-devices`
- `allow-gateway-device-rename`
- `allow-gateway-device-set-role`
- `allow-gateway-device-revoke`
- `allow-gateway-device-forget`
- `allow-gateway-audit`
- `allow-gateway-set-pin`
- `allow-gateway-pair-begin`
- `allow-gateway-pair-finish`
- `allow-gateway-pair-current`
- `allow-gateway-remote-call`
- `allow-gateway-step-up`
- `allow-gateway-link-status`
- `allow-gateway-link-retry`
- `allow-gateway-use-connection`
- `allow-move-info`
- `allow-move-export`
- `allow-move-send`
- `allow-move-cancel`
- `allow-move-remote-propose`
- `allow-move-remote-import`
- `allow-move-finish`
- `allow-move-propose`
- `allow-move-import`
- `allow-gateway-connections`
- `allow-gateway-connection-test`
- `allow-gateway-connection-forget`

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

`qs:allow-bundle-export`

</td>
<td>

Enables the bundle_export command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-bundle-export`

</td>
<td>

Denies the bundle_export command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-bundle-export-cancel`

</td>
<td>

Enables the bundle_export_cancel command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-bundle-export-cancel`

</td>
<td>

Denies the bundle_export_cancel command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-bundle-plan`

</td>
<td>

Enables the bundle_plan command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-bundle-plan`

</td>
<td>

Denies the bundle_plan command without any pre-configured scope.

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

`qs:allow-gateway-audit`

</td>
<td>

Enables the gateway_audit command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-gateway-audit`

</td>
<td>

Denies the gateway_audit command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-gateway-configure`

</td>
<td>

Enables the gateway_configure command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-gateway-configure`

</td>
<td>

Denies the gateway_configure command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-gateway-connection-forget`

</td>
<td>

Enables the gateway_connection_forget command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-gateway-connection-forget`

</td>
<td>

Denies the gateway_connection_forget command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-gateway-connection-test`

</td>
<td>

Enables the gateway_connection_test command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-gateway-connection-test`

</td>
<td>

Denies the gateway_connection_test command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-gateway-connections`

</td>
<td>

Enables the gateway_connections command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-gateway-connections`

</td>
<td>

Denies the gateway_connections command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-gateway-device-forget`

</td>
<td>

Enables the gateway_device_forget command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-gateway-device-forget`

</td>
<td>

Denies the gateway_device_forget command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-gateway-device-rename`

</td>
<td>

Enables the gateway_device_rename command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-gateway-device-rename`

</td>
<td>

Denies the gateway_device_rename command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-gateway-device-revoke`

</td>
<td>

Enables the gateway_device_revoke command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-gateway-device-revoke`

</td>
<td>

Denies the gateway_device_revoke command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-gateway-device-set-role`

</td>
<td>

Enables the gateway_device_set_role command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-gateway-device-set-role`

</td>
<td>

Denies the gateway_device_set_role command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-gateway-devices`

</td>
<td>

Enables the gateway_devices command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-gateway-devices`

</td>
<td>

Denies the gateway_devices command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-gateway-link-retry`

</td>
<td>

Enables the gateway_link_retry command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-gateway-link-retry`

</td>
<td>

Denies the gateway_link_retry command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-gateway-link-status`

</td>
<td>

Enables the gateway_link_status command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-gateway-link-status`

</td>
<td>

Denies the gateway_link_status command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-gateway-pair-begin`

</td>
<td>

Enables the gateway_pair_begin command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-gateway-pair-begin`

</td>
<td>

Denies the gateway_pair_begin command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-gateway-pair-current`

</td>
<td>

Enables the gateway_pair_current command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-gateway-pair-current`

</td>
<td>

Denies the gateway_pair_current command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-gateway-pair-finish`

</td>
<td>

Enables the gateway_pair_finish command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-gateway-pair-finish`

</td>
<td>

Denies the gateway_pair_finish command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-gateway-pairing-cancel`

</td>
<td>

Enables the gateway_pairing_cancel command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-gateway-pairing-cancel`

</td>
<td>

Denies the gateway_pairing_cancel command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-gateway-pairing-confirm`

</td>
<td>

Enables the gateway_pairing_confirm command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-gateway-pairing-confirm`

</td>
<td>

Denies the gateway_pairing_confirm command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-gateway-pairing-start`

</td>
<td>

Enables the gateway_pairing_start command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-gateway-pairing-start`

</td>
<td>

Denies the gateway_pairing_start command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-gateway-pairing-status`

</td>
<td>

Enables the gateway_pairing_status command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-gateway-pairing-status`

</td>
<td>

Denies the gateway_pairing_status command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-gateway-remote-call`

</td>
<td>

Enables the gateway_remote_call command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-gateway-remote-call`

</td>
<td>

Denies the gateway_remote_call command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-gateway-set-pin`

</td>
<td>

Enables the gateway_set_pin command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-gateway-set-pin`

</td>
<td>

Denies the gateway_set_pin command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-gateway-status`

</td>
<td>

Enables the gateway_status command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-gateway-status`

</td>
<td>

Denies the gateway_status command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-gateway-step-up`

</td>
<td>

Enables the gateway_step_up command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-gateway-step-up`

</td>
<td>

Denies the gateway_step_up command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-gateway-use-connection`

</td>
<td>

Enables the gateway_use_connection command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-gateway-use-connection`

</td>
<td>

Denies the gateway_use_connection command without any pre-configured scope.

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

`qs:allow-import-bundle`

</td>
<td>

Enables the import_bundle command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-import-bundle`

</td>
<td>

Denies the import_bundle command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-import-propose`

</td>
<td>

Enables the import_propose command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-import-propose`

</td>
<td>

Denies the import_propose command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-import-status`

</td>
<td>

Enables the import_status command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-import-status`

</td>
<td>

Denies the import_status command without any pre-configured scope.

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

`qs:allow-move-cancel`

</td>
<td>

Enables the move_cancel command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-move-cancel`

</td>
<td>

Denies the move_cancel command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-move-export`

</td>
<td>

Enables the move_export command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-move-export`

</td>
<td>

Denies the move_export command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-move-finish`

</td>
<td>

Enables the move_finish command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-move-finish`

</td>
<td>

Denies the move_finish command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-move-import`

</td>
<td>

Enables the move_import command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-move-import`

</td>
<td>

Denies the move_import command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-move-info`

</td>
<td>

Enables the move_info command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-move-info`

</td>
<td>

Denies the move_info command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-move-propose`

</td>
<td>

Enables the move_propose command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-move-propose`

</td>
<td>

Denies the move_propose command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-move-remote-import`

</td>
<td>

Enables the move_remote_import command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-move-remote-import`

</td>
<td>

Denies the move_remote_import command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-move-remote-propose`

</td>
<td>

Enables the move_remote_propose command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-move-remote-propose`

</td>
<td>

Denies the move_remote_propose command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-move-send`

</td>
<td>

Enables the move_send command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-move-send`

</td>
<td>

Denies the move_send command without any pre-configured scope.

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

`qs:allow-server-set-role`

</td>
<td>

Enables the server_set_role command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-server-set-role`

</td>
<td>

Denies the server_set_role command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-server-set-watchdog`

</td>
<td>

Enables the server_set_watchdog command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-server-set-watchdog`

</td>
<td>

Denies the server_set_watchdog command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-server-status`

</td>
<td>

Enables the server_status command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-server-status`

</td>
<td>

Denies the server_status command without any pre-configured scope.

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

`qs:allow-vault-status`

</td>
<td>

Enables the vault_status command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-vault-status`

</td>
<td>

Denies the vault_status command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:allow-vault-unlock`

</td>
<td>

Enables the vault_unlock command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`qs:deny-vault-unlock`

</td>
<td>

Denies the vault_unlock command without any pre-configured scope.

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
