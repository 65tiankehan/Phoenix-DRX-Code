#!/usr/bin/env node
/**
 * append-payload.mjs —— 把主程序产物 zip 追加到安装器 exe 尾部（自解压 trailer 模式，方案 §2.1 A）。
 * 用法：node append-payload.mjs <payloadDir> <installerExe> <outExe>
 * 载荷 trailer：[zip][u64 zip_len LE][u64 zip_offset LE]["DRXSETUP"] 共 24 字节，与 src-tauri/src/main.rs::read_payload 对应。
 * zip 用系统 PowerShell Compress-Archive 打（Windows CI/本机均有），不引第三方依赖。
 */
import { execSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'

const [payloadDir, installerExe, outExe] = process.argv.slice(2)
if (!payloadDir || !installerExe || !outExe) {
  console.error('用法：node append-payload.mjs <payloadDir> <installerExe> <outExe>')
  process.exit(1)
}

const tmpZip = path.join(path.dirname(outExe), 'payload.tmp.zip')
execSync(
  `powershell -NoProfile -Command "Compress-Archive -Force -Path '${path.resolve(payloadDir)}\\*' -DestinationPath '${path.resolve(tmpZip)}'"`,
  { stdio: 'inherit' }
)

const zip = fs.readFileSync(tmpZip)
const installer = fs.readFileSync(installerExe)
const zipOffset = installer.length
const zipLen = zip.length

const tail = Buffer.alloc(24)
tail.writeBigUInt64LE(BigInt(zipLen), 0)
tail.writeBigUInt64LE(BigInt(zipOffset), 8)
tail.write('DRXSETUP', 16, 'latin1')

fs.writeFileSync(outExe, Buffer.concat([installer, zip, tail]))
fs.rmSync(tmpZip, { force: true })
console.log(`已生成 ${outExe}（安装器 ${installer.length} B + 载荷 ${zipLen} B）`)
