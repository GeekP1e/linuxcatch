#!/usr/bin/python3
"""Explicit user-space administration model. Never changes kernel users or devices."""
import getpass
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import uuid

ROOT = Path('/workspace/lab')
STATE = ROOT / 'state.json'
USER = os.environ.get('LINUXCATCH_LAB_USER', 'student')


def fail(message):
    print('LinuxCatch model: ' + message, file=sys.stderr)
    raise SystemExit(1)


def run_child(argv, **options):
    # The child shares the foreground terminal and handles Ctrl+C itself.
    previous = signal.signal(signal.SIGINT, lambda _signal, _frame: None)
    try:
        code = subprocess.run(argv, **options).returncode
    finally:
        signal.signal(signal.SIGINT, previous)
    raise SystemExit(128 - code if code < 0 else code)


def digest(password):
    return hashlib.sha256(password.encode()).hexdigest()


def write_atomic(path, text):
    temporary = path.with_suffix('.tmp')
    temporary.write_text(text)
    temporary.replace(path)


def save(state):
    write_atomic(STATE, json.dumps(state, indent=2) + '\n')
    write_atomic(ROOT / 'etc/passwd', ''.join(
        f'{name}:x:{user["uid"]}:{user["uid"]}::{ROOT}/home/{name}:/bin/bash\n'
        for name, user in state['users'].items()))
    write_atomic(ROOT / 'etc/shadow', ''.join(
        f'{name}:{user["password"]}:0:0:99999:7:::\n'
        for name, user in state['users'].items()))


def initialize():
    (ROOT / 'etc').mkdir(parents=True, exist_ok=True)
    for name in ('root', 'student'):
        (ROOT / 'home' / name).mkdir(parents=True, exist_ok=True)
    (ROOT / 'etc/sudoers').write_text(
        'root ALL=(ALL) ALL\nstudent ALL=(ALL) NOPASSWD: ALL\n')
    (ROOT / 'etc/fstab').write_text('# Training mount table\n')
    save({'users': {'root': {'uid': 0, 'password': digest('rootpass')},
                    'student': {'uid': 1000, 'password': digest('studentpass')}},
          'devices': ['/dev/sdb1', '/dev/sdc1', '/dev/sdc2', '/dev/sdd1'],
          'mounts': {}, 'arrays': {}})


def administrator():
    if USER != 'root':
        fail('permission denied; use the training sudo or su command')


def read_password(prompt):
    if sys.stdin.isatty():
        return getpass.getpass(prompt)
    print(prompt, end='', flush=True)
    value = sys.stdin.readline()
    if not value:
        fail('password input ended')
    return value.rstrip('\r\n')


def useradd(args, state):
    administrator()
    names = [arg for arg in args if arg != '-m']
    if len(names) != 1 or any(arg.startswith('-') and arg != '-m' for arg in args):
        fail('usage: useradd [-m] USER')
    name = names[0]
    if not re.fullmatch('[a-z_][a-z0-9_-]*', name) or name in state['users']:
        fail('invalid or existing user')
    state['users'][name] = {'uid': max(1000, *(u['uid'] for u in state['users'].values())) + 1,
                            'password': '!'}
    if '-m' in args:
        (ROOT / 'home' / name).mkdir(parents=True, exist_ok=True)
    save(state)


def passwd(args, state):
    if len(args) > 1 or (args and args[0].startswith('-')):
        fail('usage: passwd [USER]')
    name = args[0] if args else USER
    if name not in state['users']:
        fail('unknown user')
    if USER != name:
        administrator()
    if USER != 'root' and digest(read_password('Current password: ')) != state['users'][name]['password']:
        fail('authentication failed')
    password = read_password('New password: ')
    if not password or password != read_password('Retype new password: '):
        fail('passwords do not match')
    state['users'][name]['password'] = digest(password)
    save(state)
    print('LinuxCatch model: password updated')


def identity_env(name, login=False):
    env = ({key: os.environ[key] for key in ('PATH', 'TERM', 'LANG', 'HISTFILE', 'HISTSIZE',
                                            'PROMPT_COMMAND') if key in os.environ}
           if login else os.environ.copy())
    env.update(LINUXCATCH_LAB_USER=name, USER=name, LOGNAME=name,
               HOME=str(ROOT / 'home' / name),
               PS1=f'{name}@linuxcatch-model:\\w$ ')
    return env


def su(args, state):
    login = False
    command = None
    name = 'root'
    user_set = False
    index = 0
    while index < len(args):
        arg = args[index]
        if arg in ('-', '-l', '--login'):
            login = True
        elif arg == '-c' and index + 1 < len(args):
            index += 1
            command = args[index]
        elif arg.startswith('-'):
            fail('unsupported su option')
        else:
            if user_set:
                fail('usage: su [-] [USER] [-c COMMAND]')
            name = arg
            user_set = True
        index += 1
    if name not in state['users']:
        fail('unknown user')
    if USER not in ('root', name):
        if digest(read_password('Password: ')) != state['users'][name]['password']:
            fail('authentication failed')
    argv = (['/bin/bash', '-c', command] if command is not None else
            ['/bin/bash', '--noprofile', '--rcfile', '/etc/linuxcatch/bashrc', '-i'])
    run_child(argv, env=identity_env(name, login),
              cwd=str(ROOT / 'home' / name) if login else None)


def sudo(args):
    if not args:
        fail('usage: sudo COMMAND [ARG ...]')
    # This model intentionally supports only the full-access rule taught by the course.
    rules = (ROOT / 'etc/sudoers').read_text().splitlines()
    if USER != 'root' and f'{USER} ALL=(ALL) NOPASSWD: ALL' not in rules:
        fail('user has no supported training sudo rule')
    run_child(args, env=identity_env('root'))


def mount(args, state):
    if not args:
        for target, entry in state['mounts'].items():
            print(f'{entry["source"]} on {target} type {entry["type"]} ({entry["options"]})')
        return
    administrator()
    entries = [line.split() for line in (ROOT / 'etc/fstab').read_text().splitlines()
               if line.strip() and not line.lstrip().startswith('#')]
    if args == ['-a']:
        selected = [row for row in entries if len(row) >= 4 and 'noauto' not in row[3].split(',')]
    elif len(args) == 1:
        selected = [row for row in entries if len(row) >= 4 and args[0] in row[:2]]
        if not selected:
            fail('mount point not found in the training fstab')
    elif len(args) == 2:
        selected = [[args[0], args[1], 'ext4', 'defaults']]
    else:
        fail('usage: mount [DEVICE TARGET | TARGET | -a]')
    pending = dict(state['mounts'])
    for source, target, fs_type, options, *_ in selected:
        if source not in state['devices'] and source not in state['arrays']:
            fail('unknown training device')
        pending[target] = {'source': source, 'type': fs_type, 'options': options}
    state['mounts'] = pending
    save(state)
    print('LinuxCatch model: mount table updated; no kernel mount performed')


def umount(args, state):
    administrator()
    if len(args) != 1 or args[0] not in state['mounts']:
        fail('training mount point not found')
    del state['mounts'][args[0]]
    save(state)
    print('LinuxCatch model: mount removed')


def mdadm(args, state):
    if '--create' in args:
        administrator()
        index = args.index('--create')
        name = args[index + 1] if index + 1 < len(args) else ''
        level = next((arg.split('=', 1)[1] for arg in args if arg.startswith('--level=')), '')
        count = next((arg.split('=', 1)[1] for arg in args if arg.startswith('--raid-devices=')), '')
        devices = [arg for arg in args[index + 2:] if not arg.startswith('-')]
        if (not name.startswith('/dev/md/') or name in state['arrays'] or level != '1'
                or count != str(len(devices)) or len(devices) < 2
                or len(set(devices)) != len(devices)
                or any(device not in state['devices'] for device in devices)
                or any(device in array['devices'] for array in state['arrays'].values() for device in devices)):
            fail('use a new training RAID 1 name and distinct unused training devices')
        raw_uuid = uuid.uuid4().hex
        state['arrays'][name] = {'level': 1, 'raid_devices': len(devices), 'devices': devices,
                                 'uuid': ':'.join(raw_uuid[i:i + 8] for i in range(0, 32, 8))}
        save(state)
        print(f'LinuxCatch model: {name} created; no kernel array created')
    elif '--add' in args:
        administrator()
        name = next((arg for arg in args if arg.startswith('/dev/md/')), '')
        index = args.index('--add')
        device = args[index + 1] if index + 1 < len(args) else ''
        if name not in state['arrays'] or device not in state['devices'] or any(
                device in array['devices'] for array in state['arrays'].values()):
            fail('unknown array, invalid device, or device already in use')
        state['arrays'][name]['devices'].append(device)
        save(state)
        print('LinuxCatch model: spare device added')
    elif '--scan' in args and any(arg in args for arg in ('--detail', '-D')):
        for name, array in state['arrays'].items():
            print(f'ARRAY {name} metadata=1.2 UUID={array["uuid"]}')
    elif any(arg in args for arg in ('--detail', '-D')):
        name = next((arg for arg in args if arg.startswith('/dev/md/')), '')
        if name not in state['arrays']:
            fail('unknown training array')
        array = state['arrays'][name]
        print(f'{name}:\n  Raid Level : raid1\n  Raid Devices : {array["raid_devices"]}'
              f'\n  Total Devices : {len(array["devices"])}\n  State : clean'
              f'\n  UUID : {array["uuid"]}\n  Devices : {" ".join(array["devices"])}')
    else:
        fail('supported: --create, --detail/-D, --detail --scan, --add')


def main():
    name = Path(sys.argv[0]).name
    args = sys.argv[1:]
    if name == 'linuxcatch-lab' and args == ['init']:
        initialize()
        return
    if not STATE.exists():
        fail('this lesson does not provide an administration model')
    if name in ('useradd', 'passwd', 'mount', 'umount', 'mdadm'):
        # Lock the complete read-modify-write transaction, not just the rename.
        with (ROOT / 'state.lock').open('a') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            globals()[name](args, json.loads(STATE.read_text()))
        return
    state = json.loads(STATE.read_text())
    if name == 'su':
        su(args, state)
    elif name == 'sudo':
        sudo(args)
    elif name == 'whoami':
        print(USER)
    elif name == 'id':
        uid = state['users'][USER]['uid']
        print(USER if args == ['-un'] else uid if args == ['-u'] else f'uid={uid}({USER}) gid={uid}({USER})')
    elif name == 'visudo':
        administrator()
        if args:
            fail('use sudo visudo; the training sudoers path is selected automatically')
        env = os.environ.copy()
        env.setdefault('EDITOR', 'nano')
        run_child(['/usr/sbin/visudo', '-f', str(ROOT / 'etc/sudoers')], env=env)
    else:
        fail('unsupported training command')


if __name__ == '__main__':
    try:
        main()
    except KeyboardInterrupt:
        print(file=sys.stderr)
        raise SystemExit(130)
    except (OSError, ValueError, KeyError) as error:
        fail(str(error))
