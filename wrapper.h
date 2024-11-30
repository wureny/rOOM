#ifndef WRAPPER_H
#define WRAPPER_H

#ifdef __APPLE__
#include <sys/types.h>
#include <signal.h>
#else
#include <sys/types.h>
#include <sys/sysinfo.h>
#include <signal.h>
#include <unistd.h>
#endif

#endif // WRAPPER_H 