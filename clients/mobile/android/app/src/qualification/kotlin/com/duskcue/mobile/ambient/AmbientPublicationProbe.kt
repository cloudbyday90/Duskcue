// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

import android.content.Context
import android.content.ContextWrapper
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger

internal class AmbientPublicationProbe(private val context: Context, private val timers: AmbientTimers) {
    fun run() {
        for (failing in listOf(false, true)) publication(failing)
        publicationFailure()
        scope()
    }
    private fun publication(failing: Boolean) {
        val lane = NativeLane("ambient-publication-probe", 1, timers)
        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        val published = AtomicBoolean()
        val cancelled = AtomicInteger()
        val work = lane.run(timeoutMs = 40, cancel = { cancelled.incrementAndGet() }, publish = { outcome ->
            check(outcome.isFailure == failing)
            entered.countDown()
            check(release.await(5, TimeUnit.SECONDS))
            published.set(true)
        }) {
            if (failing) throw AmbientFailure("controlled_release_failure")
            Unit
        }
        val initial = lane.drain()
        try {
            check(entered.await(2, TimeUnit.SECONDS))
            val fresh = lane.drain()
            check(!published.get() && !initial.isDone && !fresh.isDone && !work.actual.isDone && lane.count() == 1)
            val cancellations = cancelled.get()
            work.cancel()
            rejected { work.result.get(2, TimeUnit.SECONDS) }
            check(cancelled.get() == cancellations && !fresh.isDone && lane.count() == 1)
            rejected { lane.run { Unit } }
            rejected { lane.close() }
        } finally { release.countDown() }
        initial.get(2, TimeUnit.SECONDS)
        lane.drain().get(2, TimeUnit.SECONDS)
        check(published.get() && lane.count() == 0)
        if (failing) rejected { work.actual.get(2, TimeUnit.SECONDS) } else work.actual.get(2, TimeUnit.SECONDS)
        lane.close().get(2, TimeUnit.SECONDS)
    }
    private fun publicationFailure() {
        val lane = NativeLane("ambient-publication-failure", 1, timers)
        val original = AmbientFailure("controlled_body_failure")
        val publication = AmbientFailure("controlled_publication_failure")
        val work = lane.run<Unit>(publish = { throw publication }) { throw original }
        val failure = runCatching { work.actual.get(2, TimeUnit.SECONDS) }.exceptionOrNull()
        check(failure?.cause === original && original.suppressed.single() === publication)
        lane.drain().get(2, TimeUnit.SECONDS)
        lane.close().get(2, TimeUnit.SECONDS)
    }
    private fun scope() {
        val directory = File(context.noBackupFilesDir, "ambient-scope-probe")
        check(directory.mkdirs() || directory.isDirectory)
        val scoped = object : ContextWrapper(context) {
            override fun getNoBackupFilesDir() = directory
            override fun getApplicationContext(): Context = this
        }
        val owner = AmbientOwner(scoped)
        fun args(epoch: String) = mapOf("server_origin" to "http://127.0.0.1:12345", "bearer_token" to "scope-probe",
            "user_id" to "019a6275-6000-7000-8000-000000000001", "profile_id" to "019a6275-6000-7000-8000-000000000002", "scope_epoch" to epoch)
        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        try {
            owner.bind(args("1:1")).get(5, TimeUnit.SECONDS)
            val unchanged = owner.epochs.get()
            owner.bind(args("1:1")).get(5, TimeUnit.SECONDS)
            check(owner.epochs.get() == unchanged)
            val work = owner.commands.run { entered.countDown(); check(release.await(5, TimeUnit.SECONDS)) }
            check(entered.await(2, TimeUnit.SECONDS))
            val fenced = owner.fence(args("2:1"))
            check(owner.identity?.epoch == "2:1" && !fenced.isDone)
            rejected { owner.bind(args("1:1")) }
            val current = owner.bind(args("2:1"))
            check(!current.isDone && owner.identity?.epoch == "2:1")
            check(owner.fence(args("2:2")) === fenced)
            val latest = owner.bind(args("2:2"))
            check(owner.identity?.epoch == "2:2" && !latest.isDone)
            release.countDown()
            work.actual.get(2, TimeUnit.SECONDS)
            fenced.get(5, TimeUnit.SECONDS)
            rejected { current.get(5, TimeUnit.SECONDS) }
            latest.get(5, TimeUnit.SECONDS)
            check(owner.identity?.epoch == "2:2" && !owner.quarantined && owner.outputs.isEmpty())
            val before = owner.epochs.get()
            owner.fence(args("2:2")).get(5, TimeUnit.SECONDS)
            check(owner.epochs.get() > before)
            owner.fence(null).get(5, TimeUnit.SECONDS)
            val unready = owner.epochs.get()
            owner.fence(null).get(5, TimeUnit.SECONDS)
            check(owner.identity == null && owner.epochs.get() > unready)
            rejected { owner.bind(args("2:2")) }
        } finally {
            release.countDown()
            owner.commands.drain().get(5, TimeUnit.SECONDS)
            owner.cleanup.drain().get(5, TimeUnit.SECONDS)
            owner.journal.drain().get(5, TimeUnit.SECONDS)
            for (lane in listOf(owner.commands, owner.cleanup, owner.renewal, owner.sampling, owner.checkpoints)) lane.close().get(5, TimeUnit.SECONDS)
            owner.journal.close().get(5, TimeUnit.SECONDS)
            owner.http.drain().get(5, TimeUnit.SECONDS)
            owner.http.cancellers.close().get(5, TimeUnit.SECONDS)
            owner.timers.close().get(5, TimeUnit.SECONDS)
        }
        check(directory.listFiles().orEmpty().isEmpty() && directory.delete())
    }
}
