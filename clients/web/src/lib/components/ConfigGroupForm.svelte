<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    let { fields, valueFor, onchange } = $props();
</script>

<div class="form-grid">
    {#each fields as field}
        {#if field.type === 'boolean'}
            <label class="toggle-row">
                <input
                    type="checkbox"
                    checked={valueFor(field)}
                    onchange={(event) => onchange(field, event.currentTarget.checked)}
                />
                <span class="toggle-text">
                    <span class="toggle-title">{field.label}</span>
                    {#if field.hint}<span class="field-hint">{field.hint}</span>{/if}
                </span>
            </label>
        {:else}
            <label
                class="field"
                class:field-wide={field.type === 'list' || field.type === 'password' || field.type === 'json'}
            >
                <span class="field-label">
                    {field.label}
                    {#if field.unit}<span class="field-unit">({field.unit})</span>{/if}
                </span>
                {#if field.type === 'select' || field.type === 'select-number'}
                    <select
                        value={valueFor(field)}
                        onchange={(event) => onchange(field, field.type === 'select-number' ? Number(event.currentTarget.value) : event.currentTarget.value)}
                    >
                        {#each field.options as option}
                            <option value={option}>{option}</option>
                        {/each}
                    </select>
                {:else if field.type === 'number'}
                    <div class="number-input">
                        <input
                            type="range"
                            min={field.min}
                            max={field.max}
                            step={field.step}
                            value={valueFor(field) === '' ? field.min : valueFor(field)}
                            oninput={(event) => onchange(field, event.currentTarget.value)}
                        />
                        <input
                            type="number"
                            min={field.min}
                            max={field.max}
                            step={field.step}
                            value={valueFor(field)}
                            oninput={(event) => onchange(field, event.currentTarget.value)}
                            placeholder={field.nullable ? 'default' : ''}
                        />
                    </div>
                {:else if field.type === 'json'}
                    <textarea
                        value={valueFor(field)}
                        oninput={(event) => onchange(field, event.currentTarget.value)}
                        rows="6"
                    ></textarea>
                {:else}
                    <input
                        type={field.type === 'password' ? 'password' : 'text'}
                        value={valueFor(field)}
                        oninput={(event) => onchange(field, event.currentTarget.value)}
                    />
                {/if}
                {#if field.hint}<span class="field-hint">{field.hint}</span>{/if}
            </label>
        {/if}
    {/each}
</div>

<style>
    .form-grid {
        display: grid;
        grid-template-columns: repeat(2, minmax(0, 1fr));
        gap: 1rem;
    }

    .field,
    .toggle-row {
        display: flex;
        flex-direction: column;
        gap: 0.25rem;
        min-width: 0;
    }

    .toggle-row {
        flex-direction: row;
        align-items: flex-start;
        gap: 0.625rem;
        padding: 0.625rem 0;
    }

    .toggle-row input[type='checkbox'] {
        margin-top: 0.1875rem;
        width: auto;
    }

    .toggle-text {
        display: flex;
        flex-direction: column;
        gap: 0.125rem;
        min-width: 0;
    }

    .toggle-title {
        font-size: 0.8125rem;
        font-weight: 500;
        color: var(--color-text-primary);
    }

    .field-wide {
        grid-column: 1 / -1;
    }

    .field-label {
        font-size: 0.75rem;
        font-weight: 500;
        color: var(--color-text-secondary);
    }

    .field-unit {
        margin-inline-start: 0.25rem;
        color: var(--color-text-muted);
    }

    .field-hint {
        font-size: 0.6875rem;
        color: var(--color-text-muted);
    }

    input,
    select,
    textarea {
        width: 100%;
        min-width: 0;
        padding: 0.5rem 0.625rem;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
        background-color: var(--color-bg-elevated);
        font-size: 0.8125rem;
    }

    textarea {
        resize: vertical;
        font-family: var(--font-mono, monospace);
        line-height: 1.4;
    }

    input[type='range'] {
        padding: 0;
        border: none;
        background-color: transparent;
    }

    input:focus,
    select:focus,
    textarea:focus {
        outline: none;
        border-color: var(--color-accent);
    }

    .number-input {
        display: grid;
        grid-template-columns: minmax(0, 1fr) 110px;
        align-items: center;
        gap: 0.75rem;
    }

    @media (max-width: 700px) {
        .form-grid,
        .number-input {
            grid-template-columns: 1fr;
        }
    }
</style>
