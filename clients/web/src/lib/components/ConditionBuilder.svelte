<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import ConditionBuilder from './ConditionBuilder.svelte';

    export const CONDITION_FIELDS = [
        { field: 'video_resolution', label: m.lib_components_conditionbuilder_video_resolution(), type: 'text', placeholder: '4K, 1080P' },
        { field: 'video_codec', label: m.lib_components_conditionbuilder_video_codec(), type: 'text', placeholder: 'HEVC, H.264' },
        { field: 'video_dynamic_range', label: m.lib_components_conditionbuilder_dynamic_range(), type: 'text', placeholder: 'hdr10, dolby_vision_p5' },
        { field: 'audio_codec', label: m.lib_components_conditionbuilder_audio_codec(), type: 'text', placeholder: 'TrueHD, DTS-HD MA' },
        { field: 'audio_channels', label: m.lib_components_conditionbuilder_audio_channels(), type: 'number', placeholder: '6' },
        { field: 'container_format', label: m.lib_components_conditionbuilder_container(), type: 'text', placeholder: 'MKV, MP4' },
        { field: 'content_rating', label: m.lib_components_conditionbuilder_content_rating(), type: 'text', placeholder: 'R, PG' },
        { field: 'media_type', label: m.lib_components_conditionbuilder_media_type(), type: 'text', placeholder: 'movie, episode' },
        { field: 'genre', label: m.lib_components_conditionbuilder_genre(), type: 'text', placeholder: 'Action' },
        { field: 'critic_rating_above', label: m.lib_components_conditionbuilder_critic_rating(), type: 'number', placeholder: '8.0' },
        { field: 'has_dolby_vision', label: m.lib_components_conditionbuilder_has_dolby_vision(), type: 'boolean' },
        { field: 'has_multiple_versions', label: m.lib_components_conditionbuilder_multiple_versions(), type: 'boolean' },
        { field: 'edition', label: m.lib_components_conditionbuilder_edition(), type: 'text', placeholder: 'extended, remux' },
        { field: 'original_language', label: m.lib_components_conditionbuilder_original_language(), type: 'text', placeholder: 'en, ja' },
        { field: 'streaming_on', label: m.lib_components_conditionbuilder_streaming_on(), type: 'text', placeholder: 'netflix' },
    ];

    export const OPERATORS = [
        { op: 'eq', label: m.lib_components_conditionbuilder_equals(), valueTypes: ['text', 'number', 'boolean'] },
        { op: 'neq', label: m.lib_components_conditionbuilder_not_equals(), valueTypes: ['text', 'number', 'boolean'] },
        { op: 'in', label: m.lib_components_conditionbuilder_in_list(), valueTypes: ['text'] },
        { op: 'gt', label: m.lib_components_conditionbuilder_greater_than(), valueTypes: ['number'] },
        { op: 'gte', label: m.lib_components_conditionbuilder_greater_or_equal(), valueTypes: ['number'] },
        { op: 'lt', label: m.lib_components_conditionbuilder_less_than(), valueTypes: ['number'] },
        { op: 'lte', label: m.lib_components_conditionbuilder_less_or_equal(), valueTypes: ['number'] },
        { op: 'exists', label: m.lib_components_conditionbuilder_exists(), valueTypes: ['boolean'] },
        { op: 'matches', label: m.lib_components_conditionbuilder_matches_regex(), valueTypes: ['text'] },
    ];

    let {
        node = { operator: 'and', rules: [] },
        depth = 0,
        onchange = (n) => {},
        onremove = null,
    } = $props();

    function setOperator(op) {
        onchange({ ...node, operator: op });
    }

    function addLeaf() {
        const first = CONDITION_FIELDS[0];
        onchange({ ...node, rules: [...node.rules, { field: first.field, op: 'eq', value: '' }] });
    }

    function addGroup() {
        onchange({ ...node, rules: [...node.rules, { operator: 'and', rules: [] }] });
    }

    function updateRule(idx, next) {
        const rules = node.rules.map((r, i) => (i === idx ? next : r));
        onchange({ ...node, rules });
    }

    function removeRule(idx) {
        onchange({ ...node, rules: node.rules.filter((_, i) => i !== idx) });
    }

    function fieldMeta(field) {
        return CONDITION_FIELDS.find((f) => f.field === field) || CONDITION_FIELDS[0];
    }

    function operatorsFor(field) {
        const meta = fieldMeta(field);
        return OPERATORS.filter((o) => o.valueTypes.includes(meta.type));
    }

    function changeField(idx, field) {
        const meta = fieldMeta(field);
        const ops = operatorsFor(field);
        const op = ops[0]?.op || 'eq';
        const value = meta.type === 'boolean' ? true : meta.type === 'number' ? 0 : '';
        updateRule(idx, { field, op, value });
    }

    function changeOp(idx, op) {
        const rule = node.rules[idx];
        const meta = fieldMeta(rule.field);
        const value = meta.type === 'boolean' ? true : rule.value ?? '';
        updateRule(idx, { op, value });
    }
</script>

<div class="cond-group" style="--group-depth: {depth}">
    <div class="cond-group-header">
        <div class="match-toggle">
            <span class="match-label">{m.lib_components_conditionbuilder_match()}</span>
            <button
                type="button"
                class="seg-btn"
                class:active={node.operator === 'and'}
                onclick={() => setOperator('and')}
            >{m.lib_components_conditionbuilder_all()}</button>
            <button
                type="button"
                class="seg-btn"
                class:active={node.operator === 'or'}
                onclick={() => setOperator('or')}
            >{m.lib_components_conditionbuilder_any()}</button>
            <span class="match-label">{m.lib_components_conditionbuilder_of_the_following()}</span>
        </div>
        {#if onremove}
            <button type="button" class="cond-remove-group" title={m.lib_components_conditionbuilder_remove_group()} onclick={onremove}>✕</button>
        {/if}
    </div>

    <div class="cond-rules">
        {#each node.rules as rule, idx (idx)}
            <div class="cond-rule">
                {#if rule.operator}
                    <div class="cond-nested">
                        <ConditionBuilder
                            node={rule}
                            depth={depth + 1}
                            onchange={(n) => updateRule(idx, n)}
                            onremove={() => removeRule(idx)}
                        />
                    </div>
                {:else}
                    {@const meta = fieldMeta(rule.field)}
                    {@const ops = operatorsFor(rule.field)}
                    <select class="cond-field" value={rule.field} onchange={(e) => changeField(idx, e.currentTarget.value)}>
                        {#each CONDITION_FIELDS as f}
                            <option value={f.field}>{f.label}</option>
                        {/each}
                    </select>
                    <select class="cond-op" value={rule.op} onchange={(e) => changeOp(idx, e.currentTarget.value)}>
                        {#each ops as o}
                            <option value={o.op}>{o.label}</option>
                        {/each}
                    </select>
                    {#if rule.op === 'in'}
                        <input
                            type="text"
                            class="cond-value"
                            placeholder={m.lib_components_conditionbuilder_comma_separated_values()}
                            value={Array.isArray(rule.value) ? rule.value.join(', ') : rule.value}
                            oninput={(e) => updateRule(idx, { value: e.currentTarget.value })}
                        />
                    {:else if meta.type === 'number'}
                        <input
                            type="number"
                            step="0.1"
                            class="cond-value"
                            placeholder={meta.placeholder}
                            value={rule.value}
                            oninput={(e) => updateRule(idx, { value: parseFloat(e.currentTarget.value) || 0 })}
                        />
                    {:else if meta.type === 'boolean'}
                        {#if rule.op === 'exists'}
                            <label class="cond-bool">
                                <input
                                    type="checkbox"
                                    checked={rule.value !== false}
                                    onchange={(e) => updateRule(idx, { value: e.currentTarget.checked })}
                                />
                                <span>{rule.value !== false ? 'present' : 'absent'}</span>
                            </label>
                        {:else}
                            <select class="cond-value" value={String(rule.value)} onchange={(e) => updateRule(idx, { value: e.currentTarget.value === 'true' })}>
                                <option value="true">{m.lib_components_conditionbuilder_true()}</option>
                                <option value="false">{m.lib_components_conditionbuilder_false()}</option>
                            </select>
                        {/if}
                    {:else}
                        <input
                            type="text"
                            class="cond-value"
                            placeholder={meta.placeholder}
                            value={rule.value}
                            oninput={(e) => updateRule(idx, { value: e.currentTarget.value })}
                        />
                    {/if}
                    <button type="button" class="cond-remove" title={m.lib_components_conditionbuilder_remove_rule()} onclick={() => removeRule(idx)}>✕</button>
                {/if}
            </div>
        {/each}
    </div>

    <div class="cond-add-row">
        <button type="button" class="cond-add" onclick={addLeaf}>{m.lib_components_conditionbuilder_add_condition()}</button>
        {#if depth < 2}
            <button type="button" class="cond-add" onclick={addGroup}>{m.lib_components_conditionbuilder_add_group()}</button>
        {/if}
    </div>
</div>

<style>
    .cond-group {
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
        background-color: rgba(0, 0, 0, calc(0.08 + var(--group-depth, 0) * 0.08));
        padding: 0.75rem;
        display: flex;
        flex-direction: column;
        gap: 0.5rem;
    }

    .cond-group-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
    }

    .match-toggle {
        display: flex;
        align-items: center;
        gap: 0.375rem;
        font-size: 0.75rem;
        color: var(--color-text-secondary);
    }

    .match-label {
        color: var(--color-text-muted);
    }

    .seg-btn {
        padding: 0.125rem 0.5rem;
        font-size: 0.6875rem;
        font-weight: 600;
        color: var(--color-text-muted);
        background-color: var(--color-bg-elevated);
        border: 1px solid var(--color-border-subtle);
        border-radius: 3px;
        transition: all var(--transition-fast);
    }

    .seg-btn.active {
        color: var(--color-bg-deep);
        background-color: var(--color-accent);
        border-color: var(--color-accent);
    }

    .cond-remove-group {
        font-size: 0.75rem;
        color: var(--color-text-muted);
        padding: 0.125rem 0.375rem;
    }

    .cond-remove-group:hover {
        color: var(--color-error);
    }

    .cond-rules {
        display: flex;
        flex-direction: column;
        gap: 0.375rem;
    }

    .cond-rule {
        display: flex;
        align-items: center;
        gap: 0.375rem;
        flex-wrap: wrap;
    }

    .cond-nested {
        width: 100%;
    }

    .cond-field,
    .cond-op,
    .cond-value {
        padding: 0.25rem 0.375rem;
        background-color: var(--color-bg-elevated);
        border: 1px solid var(--color-border);
        border-radius: 3px;
        color: var(--color-text-primary);
        font-size: 0.75rem;
    }

    .cond-field {
        min-width: 130px;
    }

    .cond-op {
        min-width: 110px;
    }

    .cond-value {
        flex: 1;
        min-width: 120px;
    }

    .cond-field:focus,
    .cond-op:focus,
    .cond-value:focus {
        outline: none;
        border-color: var(--color-accent);
    }

    .cond-bool {
        display: flex;
        align-items: center;
        gap: 0.375rem;
        font-size: 0.75rem;
        color: var(--color-text-secondary);
    }

    .cond-remove {
        font-size: 0.75rem;
        color: var(--color-text-muted);
        padding: 0.125rem 0.375rem;
    }

    .cond-remove:hover {
        color: var(--color-error);
    }

    .cond-add-row {
        display: flex;
        gap: 0.5rem;
    }

    .cond-add {
        font-size: 0.6875rem;
        font-weight: 600;
        color: var(--color-accent);
        padding: 0.25rem 0.5rem;
        border: 1px dashed var(--color-border);
        border-radius: 3px;
        transition: all var(--transition-fast);
    }

    .cond-add:hover {
        border-color: var(--color-accent);
        background-color: var(--color-accent-muted);
    }
</style>
