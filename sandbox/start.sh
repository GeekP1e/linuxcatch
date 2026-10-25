#!/bin/sh
set -eu
cp -R /opt/linuxcatch/home/. /home/student/
/usr/sbin/sshd -D -e -f /etc/linuxcatch/sshd_config &
exec sleep infinity
