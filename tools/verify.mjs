import { spawnSync } from 'node:child_process';

const steps = [
  ['cargo', 'fmt', '--all', '--check'],
  ['cargo', 'clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings'],
  ['cargo', 'test', '--workspace', '--locked'],
  ['pnpm', 'contracts:check'], ['pnpm', 'boundaries:check'],
  ['pnpm', 'lint'], ['pnpm', 'typecheck'], ['pnpm', 'test'], ['pnpm', 'build'],
  ['cargo', 'build', '-p', 'collector-cli', '--locked'], ['pnpm', 'smoke'],
  ['git', 'diff', '--check'],
];
for (const [command, ...args] of steps) {
  console.log(`\n> ${command} ${args.join(' ')}`);
  // pnpm ships both JS and standalone native entrypoints; neither needs a shell.
  const pnpm = process.env.npm_execpath;
  if (command === 'pnpm' && !pnpm) throw new Error('Run this gate with pnpm verify.');
  const result = command === 'pnpm'
    ? /\.[cm]?js$/.test(pnpm)
      ? spawnSync(process.execPath, [pnpm, ...args], { stdio: 'inherit' })
      : spawnSync(pnpm, args, { stdio: 'inherit' })
    : spawnSync(command, args, { stdio: 'inherit' });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
