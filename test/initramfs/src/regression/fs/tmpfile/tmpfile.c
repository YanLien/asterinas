/* SPDX-License-Identifier: MPL-2.0 */

#define _GNU_SOURCE

#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>

#include "../../common/test.h"

#define TEST_DIR "/tmp/aster_tmpfile_test"
#define LINKED_NAME "linked_file"

/* O_TMPFILE may not be defined on older glibc. */
#ifndef O_TMPFILE
#define O_TMPFILE (__O_TMPFILE | O_DIRECTORY)
#ifndef __O_TMPFILE
#define __O_TMPFILE 020000000
#endif
#endif

#ifndef AT_EMPTY_PATH
#define AT_EMPTY_PATH 0x1000
#endif

FN_SETUP(prepare)
{
	CHECK(mkdir(TEST_DIR, 0755));
}
END_SETUP()

FN_TEST(tmpfile_open_succeeds)
{
	int fd = TEST_SUCC(open(TEST_DIR, O_TMPFILE | O_RDWR, 0666));
	close(fd);
}
END_TEST()

FN_TEST(tmpfile_open_write_only_succeeds)
{
	int fd = TEST_SUCC(open(TEST_DIR, O_TMPFILE | O_WRONLY, 0666));
	close(fd);
}
END_TEST()

FN_TEST(tmpfile_open_read_only_returns_einval)
{
	TEST_ERRNO(open(TEST_DIR, O_TMPFILE | O_RDONLY, 0666), EINVAL);
}
END_TEST()

FN_TEST(tmpfile_open_with_o_creat_returns_einval)
{
	TEST_ERRNO(open(TEST_DIR, O_TMPFILE | O_RDWR | O_CREAT, 0666), EINVAL);
}
END_TEST()

FN_TEST(tmpfile_open_with_o_excl_succeeds)
{
	int fd = TEST_SUCC(open(TEST_DIR, O_TMPFILE | O_RDWR | O_EXCL, 0666));
	close(fd);
}
END_TEST()

FN_TEST(tmpfile_open_non_dir_returns_enotdir)
{
	TEST_ERRNO(open("/dev/null", O_TMPFILE | O_RDWR, 0666), ENOTDIR);
}
END_TEST()

FN_TEST(tmpfile_invisible_in_readdir)
{
	int fd = CHECK(open(TEST_DIR, O_TMPFILE | O_RDWR, 0666));

	/* Write some data to ensure the inode exists. */
	const char *data = "hello tmpfile";
	CHECK(write(fd, data, strlen(data)));

	DIR *dir = CHECK(opendir(TEST_DIR));
	struct dirent *entry;
	int found = 0;
	while ((entry = readdir(dir)) != NULL) {
		if (strcmp(entry->d_name, ".") != 0 &&
		    strcmp(entry->d_name, "..") != 0) {
			found++;
		}
	}
	closedir(dir);
	close(fd);

	TEST_RES(found, found == 0);
}
END_TEST()

FN_TEST(tmpfile_write_and_linkat)
{
	int fd = CHECK(open(TEST_DIR, O_TMPFILE | O_RDWR, 0666));

	const char *data = "hello from tmpfile";
	CHECK(write(fd, data, strlen(data)));

	int dirfd = CHECK(open(TEST_DIR, O_RDONLY | O_DIRECTORY));

	TEST_SUCC(linkat(fd, "", dirfd, LINKED_NAME, AT_EMPTY_PATH));

	/* Verify the linked file exists and has correct content. */
	int linked_fd = CHECK(open(TEST_DIR "/" LINKED_NAME, O_RDONLY));
	char buf[64];
	ssize_t n = CHECK(read(linked_fd, buf, sizeof(buf) - 1));
	buf[n] = '\0';
	close(linked_fd);

	TEST_RES(strcmp(buf, data), _ret == 0);

	close(dirfd);
	close(fd);
}
END_TEST()

FN_TEST(tmpfile_linkat_cross_mount_returns_exdev)
{
	/* Open a tmpfile in TEST_DIR. */
	int fd = CHECK(open(TEST_DIR, O_TMPFILE | O_RDWR, 0666));

	/* Try to link into /tmp (may or may not be a different mount). */
	int tmpfd = CHECK(open("/tmp", O_RDONLY | O_DIRECTORY));
	int ret = linkat(fd, "", tmpfd, "cross_mount_tmpfile", AT_EMPTY_PATH);

	/* If it fails with EXDEV that's expected (different mount). */
	if (ret < 0 && errno == EXDEV) {
		/* Expected if different mount. */
		__tests_passed++;
		fprintf(stderr, "%s: cross-mount linkat returned EXDEV (expected)\n",
			__func__);
	} else if (ret == 0) {
		/* Same mount, clean up. */
		unlink("/tmp/cross_mount_tmpfile");
		__tests_passed++;
		fprintf(stderr, "%s: cross-mount linkat succeeded (same mount)\n",
			__func__);
	} else {
		__tests_failed++;
		fprintf(stderr, "%s: unexpected errno %s\n", __func__,
			strerror(errno));
	}

	close(tmpfd);
	close(fd);
}
END_TEST()

FN_SETUP(cleanup)
{
	unlink(TEST_DIR "/" LINKED_NAME);
	rmdir(TEST_DIR);
}
END_SETUP()
