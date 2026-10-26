# Playback reservation recovery

## Scope

The [admission foundation](../design/PLAYBACK_ADMISSION.md) separates durable viewer reservations from movie history and encoder capacity. Production playback does not attach this owner yet. These commands operate on a deliberately selected server database; they do not enable playback, restart a viewer or prove that a former execution context has stopped.

Every non-released reservation counts. A missed renewal, dead process, failed cleanup or lost response does not itself reclaim capacity. Permission/session/resource mutations record retirement transactionally; restoring the authority does not revive that reservation. A running owner can confirm its own physical completion. An uncertain former owner requires explicit local operator reconciliation.

## Identify the exact owner

Use the operator's existing database access to inspect metadata, without selecting credentials or imported feed data:

```sql
SELECT i.id, i.revision, i.fenced, i.fenced_at, i.reconciled_at,
       count(r.id) FILTER (WHERE r.state <> 'released') AS counted_reservations
FROM playback_instances i
LEFT JOIN playback_reservations r ON r.instance_id = i.id
GROUP BY i.id
ORDER BY i.created_at;
```

The internal owner also exposes its immutable instance UUIDv7 to its integrating service. Restarting creates a different instance; it never adopts the old one. There is no household-management or viewing-device recovery override.

## Fence, verify, reconcile

Use the usual configuration or `DUSKCUE_DATABASE_URL` in the command process. Avoid putting database credentials on the command line. Replace the placeholders with the exact inspected identity/revision:

```text
duskcue playback-fence --instance <instance-uuid> --revision <current-revision>
```

Successful fencing prints the exact new fence revision. The transition is permanent; a stale revision or already-fenced instance is refused. Current owner operations check the database fence. This is an authorization fence, not physical shutdown evidence.

Independently prevent the old execution context from running again, terminate its processes and verify that its media readers, sockets, child processes and output resources are gone. Do not acknowledge an instance based only on an elapsed timer, an unavailable server or the fence command's success.

Only after that verification:

```text
duskcue playback-reconcile --instance <same-instance-uuid> --fence-revision <returned-fence-revision> --resources-gone
```

The command releases only that fenced instance's remaining reservations and records a metadata-only operator event. Missing acknowledgment, an active instance or the wrong fence revision refuses the operation. A lost command response is uncertain: inspect the exact instance/revision and reservation state before taking another action. Do not substitute a new identity or assume a timeout rolled back a commit.

## Limits and next integration

Commands use a one-connection pool and bounded transactions. Reservation receipts remain durable so an old request cannot become a new viewer after release. No automated receipt pruning or general history interface is provided. Production adoption requires a bounded retention design, VOD worker/root ownership, delivery fencing and migration of every online client. Download jobs and offline local playback remain outside this online viewer ledger.
