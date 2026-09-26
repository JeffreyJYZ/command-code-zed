// Verify a published plugin release before trusting it.
//
//   bun scripts/verify-release.ts 0.3.3                    # latest check
//   bun scripts/verify-release.ts 0.3.3 --expected <sha1>  # compare to a local npm pack
//   bun scripts/verify-release.ts 0.3.3 --tries 3          # short poll (default 40 x 20s)
//
// A successful `npm publish` is asynchronous in two stages: the packument
// (dist-tags.latest + versions[<v>]) updates about a minute in, and the tarball
// URL starts serving several minutes later. During the gap `latest` can point at
// a version whose tarball still 404s — installing then yields nothing (that is
// what burned 0.2.4). This script polls until both are true, then checks the
// served bytes.
import { createHash } from "node:crypto"

const PKG = "@jeffreyjyz/opencode-command-code"
const REGISTRY = "https://registry.npmjs.org"
const encoded = PKG.replace("/", "%2F")

export interface ReleaseFacts {
	latest: string
	versionPresent: boolean
	tarballUrl: string
}

export function factsFrom(packument: unknown, version: string): ReleaseFacts {
	const body = (packument ?? {}) as {
		"dist-tags"?: Record<string, string>
		versions?: Record<string, { dist?: { tarball?: string } }>
	}
	return {
		latest: body["dist-tags"]?.latest ?? "(none)",
		versionPresent: Boolean(body.versions?.[version]),
		tarballUrl:
			body.versions?.[version]?.dist?.tarball ?? `${REGISTRY}/${encoded}/-/${PKG.split("/")[1]}-${version}.tgz`,
	}
}

/** One verdict line, so CI logs and shells read the same thing. */
export function verdict(version: string, facts: ReleaseFacts, tarballStatus: number, sha1?: string, expected?: string): string {
	const parts = [`latest=${facts.latest}`, `version=${facts.versionPresent ? "yes" : "no"}`, `tarball=${tarballStatus}`]
	if (sha1) parts.push(`sha1=${sha1}`)
	if (expected) parts.push(sha1 === expected ? "MATCH" : "MISMATCH")
	if (facts.latest !== version) parts.push(`(latest not yet moved to ${version})`)
	return parts.join(" ")
}

export function sha1Hex(bytes: ArrayBuffer | Uint8Array): string {
	return createHash("sha1").update(new Uint8Array(bytes as ArrayBuffer)).digest("hex")
}

async function main(): Promise<number> {
	const args = process.argv.slice(2)
	const version = args.find((a) => !a.startsWith("--"))
	if (!version) {
		console.error("usage: bun scripts/verify-release.ts <version> [--expected <sha1>] [--tries N]")
		return 2
	}
	const expectedIndex = args.indexOf("--expected")
	const expected = expectedIndex >= 0 ? args[expectedIndex + 1] : undefined
	const triesIndex = args.indexOf("--tries")
	const tries = triesIndex >= 0 ? Number(args[triesIndex + 1] ?? 40) : 40

	for (let i = 1; i <= tries; i++) {
		const packument = await fetch(`${REGISTRY}/${encoded}`).then((r) => r.json()).catch(() => undefined)
		const facts = factsFrom(packument, version)
		const response = await fetch(facts.tarballUrl).catch(() => undefined)
		const status = response?.status ?? 0
		if (facts.versionPresent && status === 200 && response) {
			const sha1 = sha1Hex(await response.arrayBuffer())
			const tarballVersion = await versionFromTarball(await (await fetch(facts.tarballUrl)).arrayBuffer())
			console.log(verdict(version, facts, status, sha1, expected))
			if (tarballVersion && tarballVersion !== version) {
				console.error(`tarball package.json says ${tarballVersion}, not ${version}`)
				return 1
			}
			if (expected && sha1 !== expected) {
				console.error("served bytes do not match the local pack — do not install")
				return 1
			}
			console.log(`verified ${PKG}@${version}`)
			return 0
		}
		console.log(`try=${i} ${verdict(version, facts, status)}`)
		if (i < tries) await Bun.sleep(20_000)
	}
	console.error(`timeout: ${PKG}@${version} not fully published after ${tries} tries`)
	return 1
}

/** `package/package.json` inside the tarball, without unpacking the whole thing. */
async function versionFromTarball(bytes: ArrayBuffer): Promise<string | undefined> {
	try {
		const { mkdtemp, writeFile, readFile } = await import("node:fs/promises")
		const { tmpdir } = await import("node:os")
		const { join } = await import("node:path")
		const dir = await mkdtemp(join(tmpdir(), "verify-release-"))
		const file = join(dir, "p.tgz")
		await writeFile(file, Buffer.from(bytes))
		const tar = Bun.spawnSync(["tar", "xzf", file, "-C", dir, "package/package.json"])
		if (tar.exitCode !== 0) return undefined
		const pkg = JSON.parse(await readFile(join(dir, "package", "package.json"), "utf8")) as { version?: string }
		return pkg.version
	} catch {
		return undefined
	}
}

if (import.meta.main) process.exit(await main())
