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
#define CROSS_LINK_DIR "/ext2"
#define CROSS_LINK_NAME "cross_mount_tmpfile"
#define LINKED_NAME "linked_file"
#define LINKED_O_EXCL_NAME "linked_o_excl_file"

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

static void cleanup_test_files(void)
{
	unlink(TEST_DIR "/" LINKED_NAME);
	unlink(TEST_DIR "/" LINKED_O_EXCL_NAME);
	unlink(CROSS_LINK_DIR "/" CROSS_LINK_NAME);
	rmdir(TEST_DIR);
}

static int dir_is_unavailable_or_same_mount(const char *source_path,
					    const char *target_path)
{
	struct stat source_stat;
	struct stat target_stat;

	if (stat(source_path, &source_stat) < 0 ||
	    stat(target_path, &target_stat) < 0) {
		return 1;
	}

	return source_stat.st_dev == target_stat.st_dev;
}

FN_SETUP(prepare)
{
	cleanup_test_files();
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
	DIR *dir = NULL;
	int fd = -1;

	fd = TEST_SUCC(open(TEST_DIR, O_TMPFILE | O_RDWR, 0666));
	if (fd < 0) {
		goto out;
	}

	const char *data = "hello tmpfile";
	TEST_RES(write(fd, data, strlen(data)), _ret == strlen(data));

	dir = TEST_SUCC(opendir(TEST_DIR));
	if (dir == NULL) {
		goto out;
	}

	struct dirent *entry;
	int found = 0;
	while ((entry = readdir(dir)) != NULL) {
		if (strcmp(entry->d_name, ".") != 0 &&
		    strcmp(entry->d_name, "..") != 0) {
			found++;
		}
	}
	TEST_RES(found, found == 0);
out:
	if (dir != NULL) {
		closedir(dir);
	}
	if (fd >= 0) {
		close(fd);
	}
}
END_TEST()

FN_TEST(tmpfile_write_and_linkat)
{
	int fd = -1;
	int dirfd = -1;
	int linked_fd = -1;

	fd = TEST_SUCC(open(TEST_DIR, O_TMPFILE | O_RDWR, 0666));
	if (fd < 0) {
		goto out;
	}

	const char *data = "hello from tmpfile";
	TEST_RES(write(fd, data, strlen(data)), _ret == strlen(data));

	dirfd = TEST_SUCC(open(TEST_DIR, O_RDONLY | O_DIRECTORY));
	if (dirfd < 0) {
		goto out;
	}

	if (TEST_SUCC(linkat(fd, "", dirfd, LINKED_NAME, AT_EMPTY_PATH)) < 0) {
		goto out;
	}

	struct stat stat_after_link;
	TEST_RES(fstat(fd, &stat_after_link), stat_after_link.st_nlink == 1);

	linked_fd = TEST_SUCC(open(TEST_DIR "/" LINKED_NAME, O_RDONLY));
	if (linked_fd < 0) {
		goto out;
	}

	char buf[64];
	ssize_t n = TEST_SUCC(read(linked_fd, buf, sizeof(buf) - 1));
	if (n < 0) {
		goto out;
	}
	buf[n] = '\0';

	TEST_RES(strcmp(buf, data), _ret == 0);

out:
	if (linked_fd >= 0) {
		close(linked_fd);
	}
	if (dirfd >= 0) {
		close(dirfd);
	}
	if (fd >= 0) {
		close(fd);
	}
}
END_TEST()

FN_TEST(tmpfile_open_with_o_excl_cannot_be_linked)
{
	int fd = -1;
	int dirfd = -1;

	fd = TEST_SUCC(open(TEST_DIR, O_TMPFILE | O_RDWR | O_EXCL, 0666));
	if (fd < 0) {
		goto out;
	}

	dirfd = TEST_SUCC(open(TEST_DIR, O_RDONLY | O_DIRECTORY));
	if (dirfd < 0) {
		goto out;
	}

	TEST_ERRNO(linkat(fd, "", dirfd, LINKED_O_EXCL_NAME, AT_EMPTY_PATH),
		   ENOENT);

out:
	if (dirfd >= 0) {
		close(dirfd);
	}
	if (fd >= 0) {
		close(fd);
	}
}
END_TEST()

FN_TEST(tmpfile_linkat_cross_mount_returns_exdev)
{
	SKIP_TEST_IF(
		dir_is_unavailable_or_same_mount(TEST_DIR, CROSS_LINK_DIR));

	int fd = -1;
	int tmpfd = -1;

	fd = TEST_SUCC(open(TEST_DIR, O_TMPFILE | O_RDWR, 0666));
	if (fd < 0) {
		goto out;
	}

	tmpfd = TEST_SUCC(open(CROSS_LINK_DIR, O_RDONLY | O_DIRECTORY));
	if (tmpfd < 0) {
		goto out;
	}

	TEST_ERRNO(linkat(fd, "", tmpfd, CROSS_LINK_NAME, AT_EMPTY_PATH),
		   EXDEV);

out:
	if (tmpfd >= 0) {
		close(tmpfd);
	}
	if (fd >= 0) {
		close(fd);
	}
}
END_TEST()

FN_SETUP(cleanup)
{
	cleanup_test_files();
}
END_SETUP()
