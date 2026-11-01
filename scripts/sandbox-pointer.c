// Virtual pointer for the sandbox session. A headless sway has no input
// devices, so this registers one and replays commands read from a FIFO:
//
//   move X Y W H REPLY    absolute motion to X,Y within a W×H layout
//   button CODE STATE REPLY   evdev code (272 left), 1 press / 0 release
//   scroll AXIS STEPS REPLY   axis 0 vertical / 1 horizontal, ±wheel notches
//
// Each event ends with a frame, and "ok" goes to the REPLY FIFO once the
// compositor has processed it. Exits with the compositor. Built and started by
// scripts/sandbox.sh.
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <stdio.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
#include <wayland-client.h>

// Hand-declared wlr-virtual-pointer-unstable-v1, so no protocol XML or
// generated code is needed. Request order is the opcode.
static const struct wl_message pointer_requests[] = {
    {"motion", "uff", NULL},
    {"motion_absolute", "uuuuu", NULL},
    {"button", "uuu", NULL},
    {"axis", "uuf", NULL},
    {"frame", "", NULL},
    {"axis_source", "u", NULL},
    {"axis_stop", "uu", NULL},
    {"axis_discrete", "uufi", NULL},
    {"destroy", "", NULL},
};
static const struct wl_interface pointer_interface = {
    "zwlr_virtual_pointer_v1", 1, 9, pointer_requests, 0, NULL,
};
static const struct wl_interface *create_types[] = {NULL, &pointer_interface};
static const struct wl_message manager_requests[] = {
    {"create_virtual_pointer", "?on", create_types},
};
static const struct wl_interface manager_interface = {
    "zwlr_virtual_pointer_manager_v1", 1, 1, manager_requests, 0, NULL,
};

enum { MOTION_ABSOLUTE = 1, BUTTON = 2, FRAME = 4, AXIS_SOURCE = 5, AXIS_DISCRETE = 7 };

static struct wl_proxy *manager;

static void global(void *data, struct wl_registry *registry, uint32_t name,
                   const char *interface, uint32_t version) {
    (void)data, (void)version;
    if (strcmp(interface, manager_interface.name) == 0)
        manager = wl_registry_bind(registry, name, &manager_interface, 1);
}

static void global_remove(void *data, struct wl_registry *registry, uint32_t name) {
    (void)data, (void)registry, (void)name;
}

static const struct wl_registry_listener registry_listener = {global, global_remove};

static uint32_t now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec * 1000 + ts.tv_nsec / 1000000;
}

// The caller may have died; give up rather than block forever.
static void reply(const char *path) {
    for (int i = 0; i < 200; i++) {
        int fd = open(path, O_WRONLY | O_NONBLOCK);
        if (fd >= 0) {
            write(fd, "ok\n", 3);
            close(fd);
            return;
        }
        if (errno != ENXIO) return;
        usleep(5000);
    }
}

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: sandbox-pointer FIFO\n");
        return 1;
    }
    struct wl_display *display = wl_display_connect(NULL);
    if (!display) {
        fprintf(stderr, "sandbox-pointer: cannot connect to the session\n");
        return 1;
    }
    struct wl_registry *registry = wl_display_get_registry(display);
    wl_registry_add_listener(registry, &registry_listener, NULL);
    wl_display_roundtrip(display);
    if (!manager) {
        fprintf(stderr, "sandbox-pointer: compositor lacks virtual pointers\n");
        return 1;
    }
    // A null seat means the compositor's default seat.
    struct wl_proxy *pointer =
        wl_proxy_marshal_constructor(manager, 0, &pointer_interface, NULL, NULL);
    wl_display_roundtrip(display);

    // Read-write, so the FIFO never reports EOF between writers. Unbuffered,
    // so no command waits in stdio where poll() can't see it.
    FILE *in = fopen(argv[1], "r+");
    if (!in) {
        perror("sandbox-pointer");
        return 1;
    }
    setvbuf(in, NULL, _IONBF, 0);
    struct pollfd fds[] = {{fileno(in), POLLIN, 0}, {wl_display_get_fd(display), POLLIN, 0}};
    char line[512], path[256];
    int a, b, c, d;
    while (poll(fds, 2, -1) > 0) {
        // Exit once the compositor hangs up, rather than idle forever on the FIFO.
        if (fds[1].revents && wl_display_dispatch(display) < 0) return 0;
        if (!fds[0].revents || !fgets(line, sizeof line, in)) continue;
        uint32_t t = now_ms();
        if (sscanf(line, "move %d %d %d %d %255s", &a, &b, &c, &d, path) == 5) {
            wl_proxy_marshal(pointer, MOTION_ABSOLUTE, t, a, b, c, d);
        } else if (sscanf(line, "button %d %d %255s", &a, &b, path) == 3) {
            wl_proxy_marshal(pointer, BUTTON, t, a, b);
        } else if (sscanf(line, "scroll %d %d %255s", &a, &b, path) == 3) {
            wl_proxy_marshal(pointer, AXIS_SOURCE, 0 /* wheel */);
            wl_proxy_marshal(pointer, AXIS_DISCRETE, t, a, wl_fixed_from_int(15 * b), b);
        } else {
            fprintf(stderr, "sandbox-pointer: bad command: %s", line);
            continue;
        }
        wl_proxy_marshal(pointer, FRAME);
        if (wl_display_roundtrip(display) < 0) return 1;
        reply(path);
    }
    return 0;
}
