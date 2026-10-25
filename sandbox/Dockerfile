FROM debian:bookworm-slim

RUN rm -f /etc/dpkg/dpkg.cfg.d/docker \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
       man-db manpages info nano vim-tiny less openssh-client openssh-server s-nail \
    && apt-get install -y --reinstall coreutils passwd \
    && mandb -q \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 1000 --shell /bin/bash student \
    && passwd -d student \
    && mkdir -p /workspace /run/sshd /opt/linuxcatch/home/.ssh \
    && ssh-keygen -q -t ed25519 -N '' -f /opt/linuxcatch/home/.ssh/id_ed25519 \
    && ssh-keygen -q -t ed25519 -N '' -f /opt/linuxcatch/home/.ssh/host_ed25519 \
    && cp /opt/linuxcatch/home/.ssh/id_ed25519.pub /opt/linuxcatch/home/.ssh/authorized_keys \
    && awk '{print "[127.0.0.1]:2222 " $1 " " $2}' /opt/linuxcatch/home/.ssh/host_ed25519.pub > /opt/linuxcatch/home/.ssh/known_hosts \
    && chmod 700 /opt/linuxcatch/home/.ssh \
    && chown -R student:student /workspace /opt/linuxcatch/home \
    && ln -s /usr/bin/s-nail /usr/local/bin/mail

COPY sshd_config /etc/linuxcatch/sshd_config
COPY sendmail.sh /usr/local/bin/linuxcatch-sendmail
COPY start.sh /usr/local/bin/linuxcatch-sandbox
RUN chmod 755 /usr/local/bin/linuxcatch-sendmail /usr/local/bin/linuxcatch-sandbox \
    && printf '\nset mta=/usr/local/bin/linuxcatch-sendmail\nset sendwait\n' >> /etc/s-nail.rc

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
       procps fdisk util-linux parted e2fsprogs xfsprogs sysstat lsscsi lsof kmod \
       sudo mdadm python3 \
    && rm -rf /var/lib/apt/lists/*

COPY lab.py /usr/local/bin/linuxcatch-lab
COPY bashrc /etc/linuxcatch/bashrc
RUN chmod 755 /usr/local/bin/linuxcatch-lab \
    && mkdir -p /opt/linuxcatch/lab/bin \
    && for command in useradd passwd su sudo whoami id visudo mount umount mdadm; do \
         ln -s /usr/local/bin/linuxcatch-lab /opt/linuxcatch/lab/bin/$command; \
       done

USER student
WORKDIR /workspace
ENV TERM=xterm-256color LANG=C.UTF-8
CMD ["linuxcatch-sandbox"]
