#include <linux/syscalls.h>
#include <linux/types.h>
#include <linux/module.h>
#include <linux/kernel.h>
#include <linux/init.h>
#include <linux/proc_fs.h>
#include <linux/version.h>
#include <linux/kprobes.h>

#include <asm/tlbflush.h>

#if (LINUX_VERSION_CODE >= KERNEL_VERSION(5,6,0))
static struct proc_ops fops;
#else
static struct file_operations fops;
#endif


MODULE_AUTHOR("Henry Wong <henry@stuffedcow.net>");
MODULE_DESCRIPTION("Allow retrieving CR3 and CR4 and flushing TLB.");
MODULE_LICENSE("GPL");

/*
	2020/06/07: Update to build on kernel 5.6.8
	            Use kretprobe to disable STRICT_DEVMEM check because most distributions
	            now ship with CONFIG_STRICT_DEVMEM enabled.
*/


static int kretprobe_return_one(struct kretprobe_instance *ri, struct pt_regs *regs)
{
	regs_set_return_value(regs, 1);
	return 0;
}

struct kretprobe rp = {
	.data_size = 0,
	.handler = kretprobe_return_one,
	.entry_handler = NULL,
	.kp = {
		.addr = NULL,
		.symbol_name = "devmem_is_allowed"
	},
	.maxactive = 1
};


static inline uintptr_t get_cr3(void)
{
	uintptr_t val;
	asm volatile("mov %%cr3,%0\n" : "=r"(val) : __FORCE_ORDER);
	return val;
}
static inline uintptr_t get_cr4(void)
{
	uintptr_t val;
	asm volatile("mov %%cr4,%0\n" : "=r"(val) : __FORCE_ORDER);
	return val;
}

static void do_flush_tlb_all(void *info)
{
	__flush_tlb_all();
	// native_read_cr3() disappeared starting from Kernel 4.13. It seems __flush_tlb() exists even in old kernels
	// so I'll just switch to that, but here's the original in case __flush_tlb doesn't work on some older kernels.
	// native_write_cr3(native_read_cr3());
}

static int __init pgmod_init(void)
{
	int ret;
	proc_create("pgmod", 0600, NULL, &fops);
	
	ret = register_kretprobe(&rp);
	if (ret < 0) {
		printk(KERN_INFO "pgmod: register_kretprobe of %s failed, returned %d\n", rp.kp.symbol_name, ret);
	}
	
	printk(KERN_INFO "pgmod kernel module loaded\n");
	return 0;
}

static void __exit pgmod_cleanup(void)
{
	remove_proc_entry("pgmod", NULL);
	unregister_kretprobe(&rp);
	printk(KERN_INFO "pgmod kernel module unloaded\n");
}


static int pgmod_proc_open(struct inode *sp_inode, struct file *sp_file)
{
	return 0;
}
static int pgmod_proc_release(struct inode *sp_inode, struct file *sp_file)
{
	return 0;
}
static ssize_t pgmod_proc_read(struct file *sp_file, char __user *buf, size_t size, loff_t *offset)
{
	char kbuf[32];		// Make sure the string doesn't overflow this.
	int len=0;
	
	if (*offset>=1)
	{
		return 0;
	}

	len += sprintf(kbuf+len, "%llx\n", (uint64_t)get_cr3());
	len += sprintf(kbuf+len, "%llx\n", (uint64_t)get_cr4());
	len++;	// NULL-termination
		
	if (copy_to_user(buf, kbuf, len))
		printk(KERN_INFO "copy_to_user failed");

	(*offset)++;		// Advance offset by 1 arbitrary unit.

	return len;
}
static ssize_t pgmod_proc_write(struct file *sp_file, const char __user *buf, size_t size, loff_t *offset)
{
	//printk(KERN_INFO "proc_write: Flushing all TLBs");
	on_each_cpu(do_flush_tlb_all, NULL, 1);
	return size;
}

#if (LINUX_VERSION_CODE >= KERNEL_VERSION(5,6,0))
static struct proc_ops fops = {
	.proc_open = pgmod_proc_open,
	.proc_read = pgmod_proc_read,
	.proc_write = pgmod_proc_write,
	.proc_release = pgmod_proc_release
};
#else
static struct file_operations fops = {
	.open = pgmod_proc_open,
	.read = pgmod_proc_read,
	.write = pgmod_proc_write,
	.release = pgmod_proc_release
};
#endif

module_init(pgmod_init);
module_exit(pgmod_cleanup);

