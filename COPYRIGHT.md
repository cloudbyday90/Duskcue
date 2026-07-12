Duskcue — Self-hosted media streaming server

Copyright (C) 2026 Duskcue Contributors

Duskcue is distributed under the GNU Affero General Public License, version 3 or
(at your option) any later version. See `LICENSE` for the full license text.

## Automated source-header validation

Run `node scripts/check-copyright.mjs` to validate headers and
`node scripts/update-copyright.mjs` to add or update them. Both scripts inspect
only tracked source files, leaving generated Paraglide output untouched. SQL
migrations through `20260701050000` are treated as a legacy baseline so their
content is not changed after potential application; later migrations remain
subject to the header check.
