import { test as base, expect } from '@playwright/test';
import { createApiScenario, installApiFixture } from '../fixtures/api.js';

type ScenarioOptions = {
    activeProfileId?: string;
    selectionRequired?: boolean;
    emptyCatalog?: boolean;
    authenticated?: boolean;
    browsePageSize?: number;
    userRole?: 'owner' | 'user';
};

export const test = base.extend<{
    scenarioOptions: ScenarioOptions;
    api: ReturnType<typeof createApiScenario>;
}>({
    scenarioOptions: [{}, { option: true }],
    api: async ({ page, scenarioOptions }, use) => {
        const scenario = createApiScenario(scenarioOptions);
        const pageErrors: string[] = [];
        page.on('pageerror', (error) => pageErrors.push(error.message));
        await installApiFixture(page, scenario);
        await use(scenario);
        expect(scenario.unhandledRequests, 'All API requests must have an explicit test fixture').toEqual([]);
        expect(pageErrors, 'The production route must not raise browser runtime errors').toEqual([]);
    },
});

export { expect };
